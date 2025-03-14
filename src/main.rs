use esp_idf_svc::hal;

use esp32_nimble::{
    enums::*,
    hid::*, 
    utilities::mutex::Mutex,
    BLEAdvertisementData,
    BLECharacteristic,
    BLEDevice,
    BLEHIDDevice, 
    BLEServer,
};

use std::sync::Arc;

const MOUSE_ID: u8= 0x01;

/* HID Report Descriptor for a 3-button mouse */
const HID_REPORT_DESCRIPTOR: &[u8] = hid!(
    (USAGE_PAGE, 0x01),         // USAGE_PAGE (Generic Desktop Ctrls)
    (USAGE, 0x02),              // USAGE (Mouse)
    (COLLECTION, 0x01),         // COLLECTION (Application)
    (REPORT_ID, MOUSE_ID),
    (USAGE, 0x01),              // USAGE (Pointer)
    (COLLECTION, 0x00),         // COLLECTION (Physical)
    // ------------------------------------------------- Buttons (Left, Right, Middle)
    (USAGE_PAGE, 0x09),         // USAGE_PAGE (Buttons)
    (USAGE_MINIMUM, 0x01),      // USAGE_MINIMUM (Button 1)
    (USAGE_MAXIMUM, 0x03),      // USAGE_MAXIMUM (Button 3)
    (LOGICAL_MINIMUM, 0x00),    // LOGICAL_MINIMUM (0)
    (LOGICAL_MAXIMUM, 0x01),    // LOGICAL_MAXIMUM (1)
    (REPORT_SIZE, 0x01),        // REPORT_SIZE (1) (1 bit)
    (REPORT_COUNT, 0x03),       // REPORT_COUNT (3) (3 times)
    (HIDINPUT, 0x02),           // INPUT (Data, Variable, Absolute)
    // ------------------------------------------------- Padding
    (REPORT_SIZE, 0x05),        // REPORT_SIZE (5) (5 bits)
    (REPORT_COUNT, 0x01),       // REPORT_COUNT (1) (1 time)
    (HIDINPUT, 0x03),           // INPUT (Constant, Variable, Absolute)
    // ------------------------------------------------- X/Y position
    (USAGE_PAGE, 0x01),      //    USAGE_PAGE (Generic Desktop)
    (USAGE, 0x30),           //    USAGE (X)
    (USAGE, 0x31),           //    USAGE (Y)
    (LOGICAL_MINIMUM, 0x81), //    LOGICAL_MINIMUM (-127)
    (LOGICAL_MAXIMUM, 0x7f), //    LOGICAL_MAXIMUM (127)
    (REPORT_SIZE, 0x08),     //    REPORT_SIZE (8)
    (REPORT_COUNT, 0x02),    //    REPORT_COUNT (2)
    (HIDINPUT, 0x06),        //    INPUT (Data, Variable, Relative)
    (END_COLLECTION),           // END_COLLECTION
    (END_COLLECTION),           // END_COLLECTION
);

struct MouseReport {
    buttons: u8,
    x: i8,
    y: i8,
}

/* BLE Mouse object */
struct BleMouse {
    server: &'static mut BLEServer,
    input_mouse: Arc<Mutex<BLECharacteristic>>,
    mouse_report: MouseReport,
}

impl BleMouse {
    fn new() -> anyhow::Result<Self> {
        let device = BLEDevice::take();

        device
            .security()
            .set_auth(AuthReq::all())
            .set_io_cap(SecurityIOCap::NoInputNoOutput)
            .resolve_rpa();

        let server = device.get_server();
        let mut hid = BLEHIDDevice::new(server);

        let input_mouse = hid.input_report(MOUSE_ID);

        hid.manufacturer("HaaafsCo");
        hid.pnp(0x02, 0x05ac, 0x820a, 0x0210);
        hid.hid_info(0x00, 0x02);
        hid.report_map(HID_REPORT_DESCRIPTOR);

        hid.set_battery_level(100);

        let ble_adv = device.get_advertising();

        ble_adv.lock().scan_response(false).set_data(
            BLEAdvertisementData::new()
            .name("ESP32 NES Mouse")
            .appearance(0x03C2)
            .add_service_uuid(hid.hid_service().lock().uuid()),
        )?;

        ble_adv.lock().start()?;

        Ok(Self {
            server,
            input_mouse,
            mouse_report: MouseReport {
                buttons: 0,
                x: 0,
                y: 0,
            },
        })
    }

    fn connected(&self) -> bool {
        self.server.connected_count() > 0
    }

    fn send_report(&self, report: &MouseReport) {
        self.input_mouse.lock().set_from(report).notify();
        esp_idf_svc::hal::delay::Ets::delay_ms(7);
    }
}

/* NES Controller Object */
struct NesController {
    response_time: u32,
}

impl NesController {
    fn new() -> NesController {
        NesController {
            response_time: 50,
        }
    }
}

/**
 * Function: mouse_acceleration
 * 
 * Description: A function to calculated mouse acceleration while a button is being held.
 * 
 * cur_value: input, the current value in a direction
 * direction: input, the requested new direction
 * 
 * returns: A new value for the direction
 * 
 */
fn mouse_acceleration(cur_value: &i8, direction: i8) -> i8{
    match direction {
        0 => 0,
        1 => {
            if *cur_value <= 0 {
                4
            } else {
                if *cur_value == 64 {
                    64
                } else {
                    *cur_value * 2
                }
            }
        },
        -1 => {
            if *cur_value >= 0 {
                -4
            } else {
                if *cur_value == -64 {
                    -64
                } else {
                    *cur_value * 2
                }
            }
        },
        _ => 0,
    }

}

/**
 * Function: main()
 * 
 * Initializes BLE Mouse and NES Controller peripherals, then stays in running loop that samples the controller and sends out mouse commands.
 */
fn main() -> anyhow::Result<()>{
    // It is necessary to call this function once. Otherwise some patches to the runtime
    // implemented by esp-idf-sys might not link properly. See https://github.com/esp-rs/esp-idf-template/issues/71
    esp_idf_svc::sys::link_patches();

    // Bind the log crate to the ESP Logging facilities
    esp_idf_svc::log::EspLogger::initialize_default();

    let peripherals = hal::peripherals::Peripherals::take().unwrap();

    let mut mouse = BleMouse::new()?;
    let controller = NesController::new();

    // Configure controller I/O
    let data_pin = hal::gpio::PinDriver::input(peripherals.pins.gpio4)?;
    let mut clk_pin = hal::gpio::PinDriver::output(peripherals.pins.gpio5)?;
    let mut latch_pin = hal::gpio::PinDriver::output(peripherals.pins.gpio6)?;

    clk_pin.set_low()?;
    latch_pin.set_low()?;

    loop {
        if !mouse.connected() {
            esp_idf_hal::delay::FreeRtos::delay_ms(controller.response_time);
            continue;
        }

        // Read controller
        latch_pin.toggle()?;
        latch_pin.toggle()?;

        let mut controller_input = 0;

        // Poll the controller inputs
        for i in 0..8 {
            controller_input |= (data_pin.is_low() as u16) << i;
            clk_pin.toggle()?;
            clk_pin.toggle()?;
        }

        // Alter mouse and send out result
        // A button
        if controller_input & 0x01 == 0x01 {
            mouse.mouse_report.buttons |= 0x1;

        } else {
            mouse.mouse_report.buttons &= !0x1;
        }

        // B button
        if controller_input & 0x02 == 0x02 {
            mouse.mouse_report.buttons |= 0x2;
        } else {
            mouse.mouse_report.buttons &= !0x2;
        }

        // Select button, CURRENTLY UNUSED
        // if controller_input & 0x04 == 0x04 {
        // }

        // Start button
        if controller_input & 0x08 == 0x08 {
            mouse.mouse_report.buttons |= 0x4;
        } else {
            mouse.mouse_report.buttons &= !0x4;
        }

        // Up/Down buttons
        if controller_input & 0x10 == 0x10 {
            mouse.mouse_report.y = mouse_acceleration(&mouse.mouse_report.y, -1);
        } else if controller_input & 0x20 == 0x20 {
            mouse.mouse_report.y = mouse_acceleration(&mouse.mouse_report.y, 1);
        } else {
            mouse.mouse_report.y = mouse_acceleration(&mouse.mouse_report.y, 0);
        }

        // Left/Right buttons
        if controller_input & 0x40 == 0x40 {
            mouse.mouse_report.x = mouse_acceleration(&mouse.mouse_report.x, -1);
        } else if controller_input & 0x80 == 0x80 {
            mouse.mouse_report.x = mouse_acceleration(&mouse.mouse_report.x, 1);
        } else {
            mouse.mouse_report.x = mouse_acceleration(&mouse.mouse_report.x, 0);
        }

        // Send report
        mouse.send_report(&mouse.mouse_report);

        // Wait to buffer a bit
        esp_idf_hal::delay::FreeRtos::delay_ms(controller.response_time);
    }
}
