# SolBridXML2MQTT

A Rust application that fetches XML measurement data from a Kontron Solbrid inverter and publishes it to an MQTT broker.

## Features

- 🔄 Continuous polling of XML measurement data
- 📡 MQTT publishing with configurable broker settings
- 🛡️ Robust error handling with automatic retry logic
- ⚙️ Configuration via TOML file
- 🔌 Support for multiple measurement types (AC voltage, current, power, frequency, battery data, etc.)
- 📊 Individual MQTT topics per measurement type

## Requirements

- Rust 1.70 or higher
- MQTT broker (e.g., Mosquitto, HiveMQ)
- Solar inverter or device with XML measurements endpoint

## Installation

### From Source

1. Clone the repository:
```bash
git clone https://github.com/0x6e3078/SolBridXML2MQTT.git
cd SolBridXML2MQTT
```

2. Build the project:
```bash
cargo build --release
```

3. The binary will be available at `target/release/SolBridXML2MQTT`

## Configuration

Create a `config.toml` file in the same directory as the executable:

```toml
inverter_url = "http://<inter.ip>/measurements.xml"
mqtt_broker = "<mqtt.broker.ip>"
mqtt_port = 1883
mqtt_client_id = "inverter_client"
poll_interval_secs = 1
max_errors = 40
```

### Configuration Parameters

| Parameter | Description | Default |
|-----------|-------------|---------|
| `inverter_url` | URL to the XML measurements endpoint | Required |
| `mqtt_broker` | MQTT broker hostname or IP address | Required |
| `mqtt_port` | MQTT broker port | Required |
| `mqtt_client_id` | Unique client ID for MQTT connection | Required |
| `poll_interval_secs` | Interval between polls in seconds | Required |
| `max_errors` | Maximum consecutive errors before exit | Required |

## Usage

### Running the Application

```bash
./target/release/SolBridXML2MQTT
```

Or with cargo:

```bash
cargo run --release
```

## Installation as System Service

### Quick Install
```bash
# Clone the repository
git clone https://github.com/0x6e3078/SolBridXML2MQTT.git
cd solbridxml2mqtt

# Install binary and systemd service
./install.sh
./setup-service.sh

# Edit configuration
sudo nano /etc/solbridxml2mqtt/config.toml

# Start and enable service
sudo systemctl start solbridxml2mqtt
sudo systemctl enable solbridxml2mqtt
```

### Manual Installation

See the installation scripts for detailed steps.

### Service Management
```bash
# Start service
sudo systemctl start solbridxml2mqtt

# Check status
sudo systemctl status solbridxml2mqtt

# View logs
sudo journalctl -u solbridxml2mqtt -f
```

### MQTT Topic Structure

The application publishes measurements to topics in the following format:

```
inverter/{serial_number}/{measurement_type}
```

Example topics:
- `inverter/7799ABCDEXXXXXX000/AC_Voltage1`
- `inverter/7799ABCDEXXXXXX000/AC_Power`
- `inverter/7799ABCDEXXXXXX000/BDC_BAT_Voltage`

### Payload Format

Each message contains the measurement value and unit:

```
237.3 V
```

## Supported Measurements

The application automatically publishes all measurements found in the XML data, including:

- **AC Measurements**: Voltage, Current, Power, Frequency (per phase)
- **Battery DC**: Voltage, Current, Power, State of Charge
- **DC Input**: Voltage and Current from solar panels
- **Grid Power**: Consumed, Injected, Own Consumption
- **System Status**: Derating percentage

## Error Handling

The application implements intelligent error handling:

- Network errors are logged but don't immediately terminate the application
- Up to 40 consecutive errors are tolerated before exit
- Error counter resets upon successful data retrieval
- Clear error messages with error count tracking

## Example XML Structure

The application expects XML in the following format:

```xml
<?xml version='1.0' encoding='UTF-8'?>
<root>
  <Device Name='SolBrid 10-3-4' Type='Inverter' Serial='7799ABCDEXXXXXX000' ...>
    <Measurements>
      <Measurement Value='237.3' Unit='V' Type='AC_Voltage1'/>
      <Measurement Value='382.6' Unit='W' Type='AC_Power'/>
      <!-- More measurements -->
    </Measurements>
  </Device>
</root>
```

## Development

### Dependencies

- `reqwest` - HTTP client for fetching XML data
- `serde` - Serialization framework
- `serde_xml_rs` - XML deserialization
- `tokio` - Async runtime
- `rumqttc` - MQTT client
- `toml` - Configuration file parsing

### Building for Development

```bash
cargo build
cargo run
```

### Running Tests

```bash
cargo test
```

## Troubleshooting

### Config file not found

Ensure `config.toml` is in the same directory where you run the application. The application will print the current working directory on startup.

### MQTT connection errors

- Verify the MQTT broker is running: `mosquitto -v`
- Check network connectivity to the broker
- Ensure the port is not blocked by firewall

### Inverter connection timeout

- Verify the inverter URL is accessible: `curl http://your-inverter-ip/measurements.xml`
- Check network connectivity
- Adjust firewall rules if necessary

### Too many errors

If the application exits with "Too many errors":
- Check inverter availability
- Verify network stability
- Consider increasing `max_errors` in config
- Review error logs for specific issues

## Contributing

Contributions are welcome! Please feel free to submit a Pull Request.

1. Fork the repository
2. Create your feature branch (`git checkout -b feature/AmazingFeature`)
3. Commit your changes (`git commit -m 'Add some AmazingFeature'`)
4. Push to the branch (`git push origin feature/AmazingFeature`)
5. Open a Pull Request

## License

This project is licensed under the MIT License - see the LICENSE file for details.

## Acknowledgments

- Built for SolBrid solar inverters
- Uses the excellent Rust async ecosystem
- MQTT support via rumqttc

## Author

Your Name - [@0x6e3078](https://github.com/0x6e3078)

Project Link: [https://github.com/0x6e3078/SolBridXML2MQTT](https://github.com/yourusername/SolBridXML2MQTT)