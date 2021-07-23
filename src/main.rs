#![no_std]
#![no_main]
#![feature(asm)]

use core::sync::atomic::{AtomicUsize, Ordering};
use cortex_m_rt::entry;
use defmt_rtt as _;
use panic_probe as _;
use rp2040_pac as pac;

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

    pll::PLL::new(pll_sys).configure(1, 888_000_000, 3, 1);
    pll::PLL::new(pll_usb).configure(1, 480_000_000, 5, 2);

    // Switch clk_sys to pll_sys
    clocks
        .clk_sys_ctrl
        .modify(|_, w| w.auxsrc().clksrc_pll_sys());
    clocks
        .clk_sys_ctrl
        .modify(|_, w| w.src().clksrc_clk_sys_aux());
    while clocks.clk_sys_selected.read().bits() != 2 {}
}

#[entry]
fn main() -> ! {
    let p = pac::Peripherals::take().unwrap();

    init(p.RESETS, p.WATCHDOG, p.CLOCKS, p.XOSC, p.PLL_SYS, p.PLL_USB);

    let pio = &p.PIO0;
    let instr = &pio.instr_mem;
    let ctrl: &rp2040_pac::pio0::CTRL = &pio.ctrl;
    let sm: &rp2040_pac::pio0::SM = &pio.sm[0];
    let clk_int: u16 = 128;  //65535;
    let clk_frac: u8 = 0;
    let output = 15;
    let output_pin = &p.IO_BANK0.gpio[output].gpio_ctrl;

    #[allow(clippy::unusual_byte_groupings)]
    let jmp = 0b000_00000_000_00000; // JMP 0

    instr[0].write(|w| unsafe {
        w.bits(0xe099);
        w
    });
    instr[1].write(|w| unsafe {
        w.bits(0xff01);
        w
    });

    for slot in 2..15 {
        instr[slot].write(|w| unsafe {
            w.bits(0xbf23);
            w
        });
    }

    instr[16].write(|w| unsafe {
        w.bits(0xff00);
        w
    });

    for slot in 17..29 {
        instr[slot].write(|w| unsafe {
            w.bits(0xbf23);
            w
        });
    }

    // allow LED pin to be controlled by PIO
    output_pin.write(|w| {
        w.funcsel().pio0_0();
        w.oeover().enable();
        w.outover().normal();
        w
    });

    // set PIO output pin to LED
    sm.sm_pinctrl.write(|w| unsafe {
        w.set_base().bits(output as u8);
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

    loop {
        cortex_m::asm::nop();
    }
}
