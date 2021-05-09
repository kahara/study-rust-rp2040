#![no_std]
#![no_main]
#![feature(asm)]
#![allow(unused_imports, dead_code, unused_variables)]

use core::sync::atomic::{AtomicUsize, Ordering};
use cortex_m_rt::entry;
use defmt::*;
use defmt_rtt as _;
use pac::{watchdog, xosc};
use panic_probe as _;
use rp2040_pac as pac;
use rp2040_pac::generic::Reg;
use rp2040_pac::pio0::sm::SM_CLKDIV;
use rp2040_pac::pio0::sm::SM_PINCTRL;

mod pll;
mod resets;

#[link_section = ".boot2"]
#[used]
pub static BOOT2: [u8; 256] = rp2040_boot2::BOOT_LOADER;

#[defmt::timestamp]
fn timestamp() -> u64 {
    static COUNT: AtomicUsize = AtomicUsize::new(0);
    // NOTE(no-CAS) `timestamps` runs with interrupts disabled
    let n = COUNT.load(Ordering::Relaxed);
    COUNT.store(n + 1, Ordering::Relaxed);
    n as u64
}

fn init(
    resets: pac::RESETS,
    watchdog: pac::WATCHDOG,
    clocks: pac::CLOCKS,
    xosc: pac::XOSC,
    pll_sys: pac::PLL_SYS,
    pll_usb: pac::PLL_USB,
) {
    // Now reset all the peripherals, except QSPI and XIP (we're using those
    // to execute from external flash!)

    let resets = resets::Resets::new(resets);

    // Reset everything except:
    // - QSPI (we're using it to run this code!)
    // - PLLs (it may be suicide if that's what's clocking us)
    resets.reset(!(resets::IO_QSPI | resets::PADS_QSPI | resets::PLL_SYS | resets::PLL_USB));

    resets.unreset_wait(
        resets::ALL
            & !(resets::ADC
            | resets::RTC
            | resets::SPI0
            | resets::SPI1
            | resets::UART0
            | resets::UART1
            | resets::USBCTRL),
    );

    // xosc 12 mhz
    watchdog
        .tick
        .write(|w| unsafe { w.cycles().bits(XOSC_MHZ as u16).enable().set_bit() });

    clocks.clk_sys_resus_ctrl.write(|w| unsafe { w.bits(0) });

    // Enable XOSC
    // TODO extract to HAL module
    const XOSC_MHZ: u32 = 12;
    xosc.ctrl.write(|w| w.freq_range()._1_15mhz());
    let startup_delay = (((XOSC_MHZ * 1_000_000) / 1000) + 128) / 256;
    xosc.startup
        .write(|w| unsafe { w.delay().bits(startup_delay as u16) });
    xosc.ctrl
        .write(|w| w.freq_range()._1_15mhz().enable().enable());
    while !xosc.status.read().stable().bit_is_set() {}

    // Before we touch PLLs, switch sys and ref cleanly away from their aux sources.
    clocks.clk_sys_ctrl.modify(|_, w| w.src().clk_ref());
    while clocks.clk_sys_selected.read().bits() != 1 {}
    clocks.clk_ref_ctrl.modify(|_, w| w.src().rosc_clksrc_ph());
    while clocks.clk_ref_selected.read().bits() != 1 {}

    resets.reset(resets::PLL_SYS | resets::PLL_USB);
    resets.unreset_wait(resets::PLL_SYS | resets::PLL_USB);

    pll::PLL::new(pll_sys).configure(1, 1500_000_000, 6, 2);
    pll::PLL::new(pll_usb).configure(1, 480_000_000, 5, 2);
}

#[entry]
fn main() -> ! {
    let p = pac::Peripherals::take().unwrap();

    init(p.RESETS, p.WATCHDOG, p.CLOCKS, p.XOSC, p.PLL_SYS, p.PLL_USB);

    let pio= &p.PIO0;
    let instr = &pio.instr_mem;
    let ctrl: &rp2040_pac::pio0::CTRL = &pio.ctrl;
    let sm: &rp2040_pac::pio0::SM = &pio.sm[0];
    let clk_int: u16 = 65535;
    let clk_frac:u8 = 0;
    let led_pin = 25;
    let led = &p.IO_BANK0.gpio[led_pin].gpio_ctrl;

    #[allow(clippy::unusual_byte_groupings)]
    let jmp = 0b000_00000_000_00000; // JMP 0

    instr[0].write(|w| unsafe { w.bits(0xe099); w });
    instr[1].write(|w| unsafe { w.bits(0xff01); w });
    instr[2].write(|w| unsafe { w.bits(0xff00); w });
    instr[3].write(|w| unsafe { w.bits(0x0001); w });

    // allow LED pin to be controlled by PIO
    led.write(|w| {
        w.funcsel().pio0_0();
        w.oeover().enable();
        w.outover().normal();
        w
    });

    // set PIO output pin to LED
    sm.sm_pinctrl.write(|w| unsafe {
        w.set_base().bits(led_pin as u8);
        w.set_count().bits(1);
        w
    });

    // set PIO clock divisor
    sm.sm_clkdiv.write(|w| {
        unsafe {
            w.int().bits(clk_int);
            w.frac().bits(clk_frac);
        }
        w
    });

    // restart state machine
    ctrl.write(|w| unsafe { w.sm_restart().bits(1) });

    // restart clock divisor
    ctrl.write(|w| unsafe { w.clkdiv_restart().bits(1) });

    // jump to the beginning
    sm.sm_instr.write(|w| unsafe { w.sm0_instr().bits(jmp) });

    // enable the state machine
    ctrl.write(|w| unsafe { w.sm_enable().bits(1) });

    loop {}

    //let led_pin = 25;
    //let led = &p.IO_BANK0.gpio[led_pin].gpio_ctrl;
    //loop {
    //    led.write(|w| {
    //        w.oeover().enable();
    //        w.outover().high();
    //        w
    //    });
    //    cortex_m::asm::delay(10_000);
    //   led.write(|w| {
    //        w.oeover().enable();
    //        w.outover().low();
    //        w
    //    });
    //    cortex_m::asm::delay(1_000_000);
    //}

    // ATTENTION ATTENTION ATTENTION
    // don't do this, or at least consult the datasheet before attempting to do anything with the ADC
    // fried a ~second~ third Pico board already
    //let adc = &p.ADC;
    //let bit: u16 = 0b1;
    //adc.cs.write(|w| w.start_many().set_bit());
    //loop {
    //    let result = adc.result.read().result().bits();
    //    if (0x1 & bit) != 0 {
    //        led.write(|w| {
    //            w.oeover().enable();
    //            w.outover().high();
    //            w
    //        });
    //    } else{
    //        led.write(|w| {
    //            w.oeover().enable();
    //            w.outover().low();
    //            w
    //        });
    //    }
    //}
}
