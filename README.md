# esp32-c3-nes-ble-mouse
A simple Rust project that uses the ESP32-C3 dev board to turn an NES controller into a BLE mouse

# NES controller buttons to mouse functionality

A button - Left Click

B button - Right Click

Start Button - Wheel Click

D-Pad - Directional, with mouse acceleration as the button is held

# Pin Connections
The pins of the NES controller should be connected to the ESP32C3 dev board as follows:

Data Pin -> GPIO4

CLK Pin -> GPIO5

Latch Pin -> GPIO6

5V -> 5V

GND -> GND
