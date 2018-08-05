/// RTC peripheral abstraction

// use datetime::*;
use rcc::{BDCR, APB1R1};
use pwr;
use stm32l4::stm32l4x2::{RTC};

#[derive(Clone,Copy,Debug)]
pub struct Time {
    pub hours: u8,
    pub minutes: u8,
    pub seconds: u8,
    pub daylight_savings: bool
}

impl Time {
    pub fn new(hours: u8, minutes: u8, seconds: u8, daylight_savings: bool) -> Self {
        Self {
            hours: hours,
            minutes: minutes,
            seconds: seconds,
            daylight_savings: daylight_savings
        }
    }
}

#[derive(Clone,Copy,Debug)]
pub struct Date {
    pub day: u8,
    pub date: u8,
    pub month: u8,
    pub year: u16,
}

impl Date {
    pub fn new(day: u8, date: u8, month: u8, year: u16) -> Self {
        Self {
            day: day,
            date: date,
            month: month,
            year: year
        }
    }
}

/// RTC Abstraction
pub struct Rtc {
    rtc: RTC
}

impl Rtc {
    pub fn rtc(rtc: RTC, apb1r1: &mut APB1R1, bdcr: &mut BDCR, pwrcr1: &mut pwr::CR1) -> Self {
        // enable peripheral clock for communication
        apb1r1.enr().modify(|_, w| w.rtcapben().set_bit());
        pwrcr1.reg().read(); // read to allow the pwr clock to enable
        
        pwrcr1.reg().modify(|_, w| w.dbp().set_bit());
        while pwrcr1.reg().read().dbp().bit_is_clear() {}
        
        bdcr.enr().modify(|_, w| { w.bdrst().set_bit() }); // reset
        
        bdcr.enr().modify(|_, w| unsafe {
            w.rtcsel()
                /* 
                    00: No clock
                    01: LSE oscillator clock used as RTC clock
                    10: LSI oscillator clock used as RTC clock
                    11: HSE oscillator clock divided by 32 used as RTC clock 
                */
                .bits(0b10)
                .rtcen()
                .set_bit()
                .bdrst() // reset required for clock source change
                .clear_bit()
        });


       write_protection(&rtc, false);
        {
            init_mode(&rtc, true);
            {
                rtc.cr.modify(|_, w| unsafe {
                    w.fmt()
                        .clear_bit() // 24hr
                        .osel()
                        /* 
                            00: Output disabled
                            01: Alarm A output enabled
                            10: Alarm B output enabled
                            11: Wakeup output enabled 
                        */
                        .bits(0b00)
                        .pol()
                        .clear_bit() // pol high
                });
                
                rtc.prer.modify(|_, w| unsafe {
                    w.prediv_s()
                        .bits(255)
                        .prediv_a()
                        .bits(127)
                });
            }
            init_mode(&rtc, false);

            // TODO configuration for output pins
            rtc.or.modify(|_, w| {
                w.rtc_alarm_type()
                    .clear_bit()
                    .rtc_out_rmp()
                    .clear_bit()
            });
            
        }
        write_protection(&rtc, true);

        Self {
            rtc: rtc
        }
    }

    pub fn set_time(&self, time: &Time){
        write_protection(&self.rtc, false);
        {
            init_mode(&self.rtc, true);
            {
                let (ht, hu) = byte_to_bcd2(time.hours);
                let (mnt, mnu) = byte_to_bcd2(time.minutes);
                let (st, su) = byte_to_bcd2(time.seconds);
                self.rtc.tr.write(|w| unsafe {
                    w.ht().bits(ht)
                        .hu().bits(hu)
                        .mnt().bits(mnt)
                        .mnu().bits(mnu)
                        .st().bits(st)
                        .su().bits(su)
                        .pm()
                        .clear_bit()

                });

                self.rtc.cr.modify(|_, w| {
                    w.fmt()
                        .bit(time.daylight_savings)

                });
            }
            init_mode(&self.rtc, false);
        }
        write_protection(&self.rtc, true);
    }

    pub fn get_time(&self) -> Time {
        let time;
        
        let timer = self.rtc.tr.read();
        let cr = self.rtc.cr.read();
        time = Time::new(bcd2_to_byte((timer.ht().bits(), timer.hu().bits())), 
                        bcd2_to_byte((timer.mnt().bits(), timer.mnu().bits())),
                        bcd2_to_byte((timer.st().bits(), timer.su().bits())),
                        cr.fmt().bit());
        
        write_protection(&self.rtc, true);
        
        time
    }

    pub fn set_date(&self, date: &Date){
        write_protection(&self.rtc, false);
        {
            init_mode(&self.rtc, true);
            {
                let (dt, du) = byte_to_bcd2(date.date);
                let (mt, mu) = byte_to_bcd2(date.month);
                let (yt, yu) = byte_to_bcd2((date.year - 1970_u16) as u8);

                self.rtc.dr.write(|w| unsafe {
                    w.dt().bits(dt)
                        .du().bits(du)
                        .mt().bit(mt > 0)
                        .mu().bits(mu)
                        .yt().bits(yt)
                        .yu().bits(yu)
                        .wdu().bits(date.day)
                });


            }
            init_mode(&self.rtc, false);
        }
        write_protection(&self.rtc, true);
    }

    pub fn get_date(&self) -> Date {
        let date;
        
        let dater = self.rtc.dr.read();
        date = Date::new(dater.wdu().bits(), 
                        bcd2_to_byte((dater.dt().bits(), dater.du().bits())),
                        bcd2_to_byte((dater.mt().bit() as u8, dater.mu().bits())),
                        (bcd2_to_byte((dater.yt().bits(), dater.yu().bits())) as u16 + 1970_u16) as u16);
        date
    }
    
}

fn write_protection(rtc: &RTC, enable: bool){
    if enable {
        rtc.wpr.write(|w| unsafe {
            w.bits(0xFF)
        });
    } else {
        rtc.wpr.write(|w| unsafe {
            w.bits(0xCA)
        });

        rtc.wpr.write(|w| unsafe {
            w.bits(0x53)
        });
    }
}

fn init_mode(rtc: &RTC, enabled: bool) {
    if enabled {
        let isr = rtc.isr.read();
        if isr.initf().bit_is_clear() { // are we already in init mode?
            rtc.isr.write(|w| { w.init().set_bit() });
            // rtc.isr.write(|w| unsafe { w.bits(0xFFFFFFFF) }); // Sets init mode
            while rtc.isr.read().initf().bit_is_clear() {} // wait to return to init state
        } 
    } else {
        rtc.isr.write(|w| { w.init().clear_bit() }); // Exits init mode
    }
    
}

fn byte_to_bcd2(byte: u8) -> (u8, u8){
    let mut bcd_high: u8 = 0;
    let mut value = byte;
    
    while value >= 10 {
        bcd_high += 1;
        value -= 10;
    }

    (bcd_high, ((bcd_high << 4) | value) as u8)
}

fn bcd2_to_byte(bcd: (u8, u8)) -> u8 { // TODO fix this
    let value = bcd.1 | bcd.0 << 4;
    
    let tmp = ((value & 0xF0) >> 0x4) * 10;
    
    (tmp + (value & 0x0F))
}