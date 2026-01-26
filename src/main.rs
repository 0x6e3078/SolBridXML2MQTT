// SolBridXML2MQTT - A Rust application for Kontron Solbrid inverter data to MQTT.

use futures::stream;
use influxdb2::{Client as InfluxClient, models::DataPoint};
use reqwest::Client;
use rumqttc::{AsyncClient, MqttOptions, QoS};
use serde::{Deserialize, Serialize};
use serde_xml_rs::from_str;
use std::collections::HashMap;
use std::fs;
use std::time::Duration;
use tokio::time::sleep;

const HTTP_TIMEOUT_SECS: u64 = 5;

// --- Home Assistant Discovery Structs ---

#[derive(Serialize, Debug)]
struct HaDiscovery {
    name: String,
    unique_id: String,
    state_topic: String,
    value_template: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    unit_of_measurement: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    device_class: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    state_class: Option<String>,
    device: HaDevice,
}

#[derive(Serialize, Debug)]
struct HaDevice {
    identifiers: Vec<String>,
    name: String,
    manufacturer: String,
    model: String,
}

// --- Data Collection Structs ---

#[derive(Serialize, Debug)]
struct InverterJson {
    device_name: String,
    serial: String,
    measurements: HashMap<String, Option<f64>>,
}

impl InverterJson {
    fn new() -> Self {
        InverterJson {
            device_name: String::new(),
            serial: String::new(),
            measurements: HashMap::new(),
        }
    }
}

// --- Configuration Structs ---

#[derive(Debug, Deserialize)]
struct Config {
    inverter_url: String,
    poll_interval_secs: u64,
    max_errors: u32,
    quiet_mode: Option<bool>,
    mqtt: Option<MqttConfig>,
    influxdb: Option<InfluxDbConfig>,
}

#[derive(Debug, Deserialize)]
struct MqttConfig {
    broker: String,
    port: u16,
    client_id: String,
    use_json: Option<bool>,
    // New: Prefix for HA discovery (default: "homeassistant")
    ha_discovery_prefix: Option<String>,
}

#[derive(Debug, Deserialize)]
struct InfluxDbConfig {
    url: String,
    token: String,
    org: String,
    bucket: String,
}

// --- XML Parsing Structs ---

#[derive(Debug, Deserialize)]
struct Root {
    #[serde(rename = "Device")]
    device: Device,
}

#[derive(Debug, Deserialize)]
struct Device {
    #[serde(rename = "@Name")]
    name: String,
    #[serde(rename = "@Serial")]
    serial: String,
    #[serde(rename = "Measurements")]
    measurements: Measurements,
}

#[derive(Debug, Deserialize)]
struct Measurements {
    #[serde(rename = "Measurement")]
    measurement: Vec<Measurement>,
}

#[derive(Debug, Deserialize)]
struct Measurement {
    #[serde(rename = "@Value")]
    value: Option<String>,
    #[serde(rename = "@Type")]
    typ: String,
    #[serde(rename = "@Unit")]
    unit: Option<String>,
}

fn parse_value(value: &str) -> Option<f64> {
    if value.eq_ignore_ascii_case("nan") || value.is_empty() {
        None
    } else {
        value.parse::<f64>().ok()
    }
}

// Helper to guess HA Device Class based on unit
fn map_device_class(unit: &str) -> (Option<String>, Option<String>) {
    match unit {
        "V" => (Some("voltage".to_string()), Some("measurement".to_string())),
        "A" => (Some("current".to_string()), Some("measurement".to_string())),
        "W" => (Some("power".to_string()), Some("measurement".to_string())),
        "Hz" => (Some("frequency".to_string()), Some("measurement".to_string())),
        "kWh" | "Wh" => (Some("energy".to_string()), Some("total_increasing".to_string())),
        "%" => (Some("battery".to_string()), Some("measurement".to_string())), // Assuming SoC or similar
        "C" | "°C" => (Some("temperature".to_string()), Some("measurement".to_string())),
        _ => (None, None),
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // --- Config Loading ---
    let config_paths = ["config.toml", "/etc/solbridxml2mqtt/config.toml"];
    let mut config_str = None;
    let mut used_path = String::new();

    for path in &config_paths {
        if let Ok(content) = fs::read_to_string(path) {
            config_str = Some(content);
            used_path = path.to_string();
            break;
        }
    }

    let config: Config = toml::from_str(
        config_str.ok_or("Could not find config.toml")?.as_str()
    ).map_err(|e| format!("Failed to parse config: {}", e))?;

    let quiet_mode = config.quiet_mode.unwrap_or(false);

    // --- Setup Clients ---
    let http_client = Client::builder()
        .timeout(Duration::from_secs(HTTP_TIMEOUT_SECS))
        .build()?;

    let mqtt_client_option = if let Some(mqtt_conf) = &config.mqtt {
        if !quiet_mode {
            println!("MQTT Active: {}:{}", mqtt_conf.broker, mqtt_conf.port);
        }
        let mut mqttoptions = MqttOptions::new(&mqtt_conf.client_id, &mqtt_conf.broker, mqtt_conf.port);
        mqttoptions.set_keep_alive(Duration::from_secs(5));

        let (mqtt_client, mut eventloop) = AsyncClient::new(mqttoptions, 10);
        tokio::spawn(async move {
            loop {
                if let Err(e) = eventloop.poll().await {
                    eprintln!("MQTT Eventloop Error: {:?}", e);
                    sleep(Duration::from_secs(1)).await;
                }
            }
        });
        Some(mqtt_client)
    } else {
        None
    };

    let influx_client_option = if let Some(influx_conf) = &config.influxdb {
        if !quiet_mode { println!("InfluxDB Active: {}", influx_conf.url); }
        Some(InfluxClient::new(&influx_conf.url, &influx_conf.org, &influx_conf.token))
    } else {
        None
    };

    if !quiet_mode {
        println!("--- Startup ---");
        println!("Inverter: {}", config.inverter_url);
    }

    let mut error_count = 0;
    // Flag to ensure we only publish discovery config once per run
    let mut discovery_done = false;

    loop {
        match http_client.get(&config.inverter_url).send().await {
            Ok(resp) => {
                match resp.text().await {
                    Ok(xml_str) => {
                        match from_str::<Root>(&xml_str) {
                            Ok(root) => {
                                error_count = 0;
                                let device_serial = &root.device.serial;
                                let device_name = &root.device.name;

                                // --- MQTT Logic ---
                                if let Some(mqtt_client) = &mqtt_client_option {
                                    let mqtt_conf = config.mqtt.as_ref().unwrap();
                                    let use_json = mqtt_conf.use_json.unwrap_or(false);

                                    // 1. Home Assistant Auto Discovery (Runs Once)
                                    // Only works effectively if use_json is true
                                    if use_json && !discovery_done {
                                        let prefix = mqtt_conf.ha_discovery_prefix.as_deref().unwrap_or("homeassistant");
                                        let status_topic = format!("solbrid/{}/status", device_serial);

                                        println!("Sending Home Assistant Discovery Config...");

                                        for m in &root.device.measurements.measurement {
                                            let safe_key = m.typ.replace(" ", "_").replace(".", "");
                                            let unit = m.unit.clone();
                                            let (dev_class, state_class) = map_device_class(unit.as_deref().unwrap_or(""));

                                            // Configuration Topic: homeassistant/sensor/<node_id>/<object_id>/config
                                            let config_topic = format!("{}/sensor/solbrid_{}/{}/config",
                                                prefix, device_serial, safe_key);

                                            let discovery_payload = HaDiscovery {
                                                name: m.typ.replace("_", " "), // Prettier name
                                                unique_id: format!("solbrid_{}_{}", device_serial, safe_key),
                                                state_topic: status_topic.clone(),
                                                // Extract value from JSON using Jinja2 template
                                                value_template: format!("{{{{ value_json.measurements.{} }}}}", m.typ),
                                                unit_of_measurement: unit,
                                                device_class: dev_class,
                                                state_class: state_class,
                                                device: HaDevice {
                                                    identifiers: vec![device_serial.clone()],
                                                    name: device_name.clone(),
                                                    manufacturer: "Kontron/Steca".to_string(),
                                                    model: "SolBrid".to_string(),
                                                },
                                            };

                                            if let Ok(payload) = serde_json::to_string(&discovery_payload) {
                                                let _ = mqtt_client.publish(&config_topic, QoS::AtLeastOnce, true, payload).await;
                                            }
                                        }
                                        discovery_done = true;
                                    }

                                    // 2. Publish Data
                                    if use_json {
                                        // JSON Mode
                                        let mut json_data = InverterJson::new();
                                        json_data.device_name = device_name.clone();
                                        json_data.serial = device_serial.clone();

                                        for m in &root.device.measurements.measurement {
                                            if let Some(val_str) = &m.value {
                                                json_data.measurements.insert(m.typ.clone(), parse_value(val_str));
                                            }
                                        }

                                        let topic = format!("solbrid/{}/status", device_serial);
                                        if let Ok(payload) = serde_json::to_string(&json_data) {
                                            let _ = mqtt_client.publish(topic, QoS::AtLeastOnce, false, payload).await;
                                            if !quiet_mode { println!("JSON Published"); }
                                        }
                                    } else {
                                        // Legacy/Individual Topic Mode
                                        for m in &root.device.measurements.measurement {
                                            if let Some(val_str) = &m.value {
                                                let topic = format!("solbrid/{}/{}", device_serial, m.typ);
                                                let payload = format!("{} {}", val_str, m.unit.as_deref().unwrap_or("")).trim().to_string();
                                                let _ = mqtt_client.publish(topic, QoS::AtLeastOnce, false, payload).await;
                                            }
                                        }
                                    }
                                }

                                // --- InfluxDB Logic (Unchanged) ---
                                if let Some(influx_client) = &influx_client_option {
                                    let mut points = Vec::new();
                                    for m in &root.device.measurements.measurement {
                                        if let Some(val_str) = &m.value {
                                            if let Some(val) = parse_value(val_str) {
                                                let mut b = DataPoint::builder("inverter_data")
                                                    .tag("serial", device_serial.as_str())
                                                    .tag("type", m.typ.as_str())
                                                    .field("value", val);
                                                if let Some(u) = &m.unit { b = b.tag("unit", u.as_str()); }
                                                if let Ok(p) = b.build() { points.push(p); }
                                            }
                                        }
                                    }
                                    if !points.is_empty() {
                                        let bucket = &config.influxdb.as_ref().unwrap().bucket;
                                        let _ = influx_client.write(bucket, stream::iter(points)).await;
                                    }
                                }
                            },
                            Err(e) => eprintln!("XML Parse Error: {:?}", e),
                        }
                    },
                    Err(e) => eprintln!("Response Error: {:?}", e),
                }
            },
            Err(e) => eprintln!("Connection Error: {:?}", e),
        }

        sleep(Duration::from_secs(config.poll_interval_secs)).await;
    }
}