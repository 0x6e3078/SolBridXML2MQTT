// SolBridXML2MQTT - A Rust application for Kontron Solbrid inverter data to MQTT.
// Copyright (C) <YEAR>  <YOUR NAME>
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.
//
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the
// GNU General Public License for more details.
//
// You should have received a copy of the GNU General Public License
// along with this program. If not, see <http://www.gnu.org/licenses/>.

use reqwest::Client;
use serde_xml_rs::from_str;
use std::time::Duration;
use serde::Deserialize;
use tokio::time::sleep;
use rumqttc::{MqttOptions, AsyncClient, QoS};
use std::fs;
use std::env;

#[derive(Debug, Deserialize)]
struct Config {
    inverter_url: String,
    mqtt_broker: String,
    mqtt_port: u16,
    mqtt_client_id: String,
    poll_interval_secs: u64,
    max_errors: u32,
}

#[derive(Debug, Deserialize)]
struct Root {
    #[serde(rename = "Device")]
    device: Device,
}

#[derive(Debug, Deserialize)]
struct Device {
    #[serde(rename = "@Name")]
    name: String,
    #[serde(rename = "@Type")]
    _device_type: String,
    #[serde(rename = "@Serial")]
    serial: String,
    #[serde(rename = "@IpAddress")]
    _ip_address: String,
    #[serde(rename = "@DateTime")]
    _date_time: String,
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

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Try multiple config locations
    let config_paths = [
        "config.toml",                          // Current directory (for development)
        "/etc/solbridxml2mqtt/config.toml",     // System-wide (for service)
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
        .ok_or("Could not find config.toml in any of these locations: config.toml, /etc/solbridxml2mqtt/config.toml")?;

    println!("Using configuration from: {}", used_path);

    let config: Config = toml::from_str(&config_str)
        .map_err(|e| format!("Failed to parse config.toml: {}", e))?;
    // Print current working directory for debugging
    let current_dir = env::current_dir()?;
    println!("Current working directory: {:?}", current_dir);

    println!("Configuration loaded:");
    println!("  Inverter URL: {}", config.inverter_url);
    println!("  MQTT Broker: {}:{}", config.mqtt_broker, config.mqtt_port);
    println!("  Poll Interval: {}s", config.poll_interval_secs);
    println!("  Max Errors: {}", config.max_errors);

    let client = Client::new();

    // Setup MQTT connection with config values
    let mut mqttoptions = MqttOptions::new(
        &config.mqtt_client_id,
        &config.mqtt_broker,
        config.mqtt_port
    );
    mqttoptions.set_keep_alive(Duration::from_secs(5));

    let (mqtt_client, mut eventloop) = AsyncClient::new(mqttoptions, 10);

    // Spawn a task to handle MQTT eventloop
    tokio::spawn(async move {
        loop {
            if let Err(e) = eventloop.poll().await {
                eprintln!("MQTT Error: {:?}", e);
                sleep(Duration::from_secs(1)).await;
            }
        }
    });

    let mut error_count = 0;

    loop {
        match client.get(&config.inverter_url).send().await {
            Ok(resp) => {
                match resp.text().await {
                    Ok(xml_str) => {
                        match from_str::<Root>(&xml_str) {
                            Ok(root) => {
                                // Reset error counter on success
                                error_count = 0;

                                println!("Device: {:?}", root.device.name);

                                // Publish each measurement to MQTT
                                for measurement in &root.device.measurements.measurement {
                                    if let Some(value) = &measurement.value {
                                        let topic = format!("inverter/{}/{}", root.device.serial, measurement.typ);
                                        let payload = if let Some(unit) = &measurement.unit {
                                            format!("{} {}", value, unit)
                                        } else {
                                            value.clone()
                                        };

                                        if let Err(e) = mqtt_client
                                            .publish(&topic, QoS::AtLeastOnce, false, payload.as_bytes())
                                            .await {
                                            eprintln!("MQTT Publish Error: {:?}", e);
                                        } else {
                                            println!("Published: {} = {}", topic, payload);
                                        }
                                    }
                                }
                            }
                            Err(e) => {
                                error_count += 1;
                                eprintln!("XML Parse Error ({}/{}): {:?}", error_count, config.max_errors, e);
                                if error_count >= config.max_errors {
                                    return Err(format!("Too many XML parse errors: {}", e).into());
                                }
                            }
                        }
                    }
                    Err(e) => {
                        error_count += 1;
                        eprintln!("Response Text Error ({}/{}): {:?}", error_count, config.max_errors, e);
                        if error_count >= config.max_errors {
                            return Err(format!("Too many response text errors: {}", e).into());
                        }
                    }
                }
            }
            Err(e) => {
                error_count += 1;
                eprintln!("Request Error ({}/{}): {:?}", error_count, config.max_errors, e);
                if error_count >= config.max_errors {
                    return Err(format!("Too many request errors: {}", e).into());
                }
            }
        }

        sleep(Duration::from_secs(config.poll_interval_secs)).await;
    }
}