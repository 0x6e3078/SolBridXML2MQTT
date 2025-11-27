// SolBridXML2MQTT - A Rust application for Kontron Solbrid inverter data to MQTT.

use futures::stream;
use influxdb2::{models::DataPoint, Client as InfluxClient};
use reqwest::Client;
use rumqttc::{AsyncClient, MqttOptions, QoS};
use serde::Deserialize;
use serde_xml_rs::from_str;
use std::fs;
use std::time::Duration;
use tokio::time::sleep;

const HTTP_TIMEOUT_SECS: u64 = 5;

// --- New Nested Configuration Structs ---

#[derive(Debug, Deserialize)]
struct Config {
    inverter_url: String,
    poll_interval_secs: u64,
    max_errors: u32,
    quiet_mode: Option<bool>,

    // These sections are Optional.
    // If [mqtt] is missing in TOML, this field will be None.
    mqtt: Option<MqttConfig>,
    influxdb: Option<InfluxDbConfig>,
}

#[derive(Debug, Deserialize)]
struct MqttConfig {
    broker: String,
    port: u16,
    client_id: String,
}

#[derive(Debug, Deserialize)]
struct InfluxDbConfig {
    url: String,
    token: String,
    org: String,
    bucket: String,
}

// --- XML Parsing Structs (Unchanged) ---

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
    value.parse::<f64>().ok()
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // --- Configuration Loading ---
    let config_paths = [
        "config.toml",
        "/etc/solbridxml2mqtt/config.toml",
    ];

    let mut config_str = None;
    let mut used_path = String::new();

    for path in &config_paths {
        if let Ok(content) = fs::read_to_string(path) {
            config_str = Some(content);
            used_path = path.to_string();
            break;
        }
    }

    let config_str = config_str
        .ok_or("Could not find config.toml in any of these locations.")?;

    let config: Config = toml::from_str(&config_str)
        .map_err(|e| format!("Failed to parse config.toml: {}", e))?;

    let quiet_mode = config.quiet_mode.unwrap_or(false);

    // --- Client Initialization ---

    let http_client = Client::builder()
        .timeout(Duration::from_secs(HTTP_TIMEOUT_SECS))
        .build()?;

    // MQTT Client Setup
    // Now we just check if the `config.mqtt` struct exists
    let mqtt_client_option = if let Some(mqtt_conf) = &config.mqtt {
        if !quiet_mode {
            println!("MQTT Configuration found: {}:{}", mqtt_conf.broker, mqtt_conf.port);
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

    // InfluxDB Client Setup
    let influx_client_option = if let Some(influx_conf) = &config.influxdb {
        if !quiet_mode {
            println!("InfluxDB Configuration found: {}", influx_conf.url);
        }
        Some(InfluxClient::new(
            &influx_conf.url,
            &influx_conf.org,
            &influx_conf.token,
        ))
    } else {
        None
    };

    if mqtt_client_option.is_none() && influx_client_option.is_none() {
        return Err("No valid MQTT or InfluxDB configuration found. Please check your config.toml.".into());
    }

    if !quiet_mode {
        println!("--- Startup Configuration ---");
        println!("Using configuration from: {}", used_path);
        println!("Inverter URL: {}", config.inverter_url);
        println!("Poll Interval: {}s", config.poll_interval_secs);
        println!("-----------------------------");
    }

    let mut error_count = 0;

    loop {
        match http_client.get(&config.inverter_url).send().await {
            Ok(resp) => {
                match resp.text().await {
                    Ok(xml_str) => {
                        match from_str::<Root>(&xml_str) {
                            Ok(root) => {
                                error_count = 0;
                                let device_serial = &root.device.serial;

                                if !quiet_mode {
                                    println!("Device: {:?}", root.device.name);
                                }

                                let mut influx_points: Vec<DataPoint> = Vec::new();

                                for measurement in &root.device.measurements.measurement {
                                    if let Some(value_str) = &measurement.value {
                                        let measurement_name = &measurement.typ;
                                        let unit_str = measurement.unit.as_deref().unwrap_or("");

                                        // 1. MQTT Publish
                                        if let Some(mqtt_client) = &mqtt_client_option {
                                            let topic = format!("inverter/{}/{}", device_serial, measurement_name);
                                            let payload = format!("{} {}", value_str, unit_str).trim().to_string();

                                            if let Err(e) = mqtt_client
                                                .publish(&topic, QoS::AtLeastOnce, false, payload.as_bytes())
                                                .await {
                                                eprintln!("MQTT Publish Error: {:?}", e);
                                            } else {
                                                if !quiet_mode {
                                                    println!("MQTT Published: {} = {}", topic, payload);
                                                }
                                            }
                                        }

                                        // 2. InfluxDB Point Preparation
                                        if let Some(_influx_client) = &influx_client_option {
                                            if let Some(value) = parse_value(value_str) {
                                                let mut builder = DataPoint::builder("inverter_data")
                                                    .tag("serial", device_serial.as_str())
                                                    .tag("type", measurement_name.as_str())
                                                    .field("value", value);

                                                if let Some(unit) = &measurement.unit {
                                                    builder = builder.tag("unit", unit.as_str());
                                                }

                                                if let Ok(point) = builder.build() {
                                                    influx_points.push(point);
                                                }
                                            }
                                        }
                                    }
                                }

                                // 3. InfluxDB Write Batch
                                // We access the bucket from the struct now: config.influxdb.as_ref().unwrap().bucket
                                if let Some(influx_client) = &influx_client_option {
                                    if !influx_points.is_empty() {
                                        // Safe to unwrap here because we know influx_client_option is Some
                                        let bucket = &config.influxdb.as_ref().unwrap().bucket;

                                        let points_stream = stream::iter(influx_points);

                                        match influx_client.write(bucket, points_stream).await {
                                            Ok(_) => {
                                                if !quiet_mode {
                                                    println!("InfluxDB Write Success");
                                                }
                                            },
                                            Err(e) => {
                                                error_count += 1;
                                                eprintln!("InfluxDB Write Error: {:?}", e);
                                            }
                                        }
                                    }
                                }
                            }
                            Err(e) => {
                                error_count += 1;
                                eprintln!("XML Parse Error: {:?}", e);
                            }
                        }
                    }
                    Err(e) => eprintln!("Response Text Error: {:?}", e),
                }
            }
            Err(e) => {
                error_count += 1;
                eprintln!("Request Error: {:?}", e);
            }
        }

        if error_count >= config.max_errors {
            return Err(format!("Too many errors ({}), stopping.", error_count).into());
        }

        sleep(Duration::from_secs(config.poll_interval_secs)).await;
    }
}