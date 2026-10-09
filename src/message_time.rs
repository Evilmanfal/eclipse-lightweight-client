//! Local calendar dates and bounded timestamp labels, using Windows' timezone/DST rules.
use std::{collections::HashMap, time::{Duration, Instant}};
use windows_sys::Win32::{Foundation::{FILETIME,SYSTEMTIME},System::{
    SystemInformation::{GetLocalTime,GetSystemTime},
    Time::{FileTimeToSystemTime,SystemTimeToFileTime,SystemTimeToTzSpecificLocalTimeEx},
}};

pub struct Clock {
    day:[u16;3],
    checked:Instant,
    cleared:Instant,
    labels:HashMap<String,String>,
}
impl Default for Clock {
    fn default()->Self {Self{day:today(),checked:Instant::now(),cleared:Instant::now(),labels:HashMap::new()}}
}
impl Clock {
    pub fn label(&mut self,timestamp:&str)->String {
        if self.checked.elapsed()>=Duration::from_secs(1){
            let day=today();
            if self.day!=day||self.cleared.elapsed()>=Duration::from_secs(60){self.day=day;self.labels.clear();self.cleared=Instant::now();}
            self.checked=Instant::now();
        }
        if let Some(label)=self.labels.get(timestamp){return label.clone();}
        let label=parse_utc(timestamp).and_then(|utc|{
            let mut local=unsafe{std::mem::zeroed()};
            // A null timezone asks Windows for the current user's timezone, with rules for this date.
            (unsafe{SystemTimeToTzSpecificLocalTimeEx(std::ptr::null(),&utc,&mut local)}!=0).then(||format_at(&local,self.day))
        }).unwrap_or_else(||timestamp.chars().take(40).collect());
        if timestamp.len()<=80{
            if self.labels.len()>=256{self.labels.clear();}
            self.labels.insert(timestamp.into(),label.clone());
        }
        label
    }
}
fn today()->[u16;3]{let mut local=unsafe{std::mem::zeroed()};unsafe{GetLocalTime(&mut local)};[local.wYear,local.wMonth,local.wDay]}
fn format_at(local:&SYSTEMTIME,today:[u16;3])->String {
    let time=format!("{:02}:{:02}",local.wHour,local.wMinute);
    if [local.wYear,local.wMonth,local.wDay]<today{
        let month=["Jan","Feb","Mar","Apr","May","Jun","Jul","Aug","Sep","Oct","Nov","Dec"][(local.wMonth.clamp(1,12)-1)as usize];
        format!("{time} · {month} {}, {}",local.wDay,local.wYear)
    }else{time}
}
fn from_ticks(ticks:u64)->Option<SYSTEMTIME>{
    let file=FILETIME{dwLowDateTime:ticks as u32,dwHighDateTime:(ticks>>32)as u32};let mut time=unsafe{std::mem::zeroed()};
    (unsafe{FileTimeToSystemTime(&file,&mut time)}!=0).then_some(time)
}
fn to_ticks(time:&SYSTEMTIME)->Option<u64>{
    let mut file=unsafe{std::mem::zeroed()};
    if unsafe{SystemTimeToFileTime(time,&mut file)}==0{return None;}
    Some((u64::from(file.dwHighDateTime)<<32)|u64::from(file.dwLowDateTime))
}
fn parse_utc(s:&str)->Option<SYSTEMTIME>{
    if s.len()<20||s.len()>80||!s.is_ascii(){return None;}
    let bytes=s.as_bytes();if bytes[4]!=b'-'||bytes[7]!=b'-'||!matches!(bytes[10],b'T'|b't')||bytes[13]!=b':'||bytes[16]!=b':'{return None;}
    let number=|a:usize,b:usize|{let part=s.get(a..b)?;if !part.bytes().all(|b|b.is_ascii_digit()){return None;}part.parse::<u16>().ok()};
    let(year,month,day,hour,minute,second)=(number(0,4)?,number(5,7)?,number(8,10)?,number(11,13)?,number(14,16)?,number(17,19)?);
    if year<1601||!(1..=12).contains(&month)||hour>23||minute>59||second>59{return None;}
    let leap=year%4==0&&(year%100!=0||year%400==0);let days=[31,if leap{29}else{28},31,30,31,30,31,31,30,31,30,31];
    if day==0||day>days[month as usize-1]{return None;}
    let mut end=19;let mut milliseconds=0;
    if bytes.get(end)==Some(&b'.'){
        end+=1;let start=end;while bytes.get(end).is_some_and(u8::is_ascii_digit){end+=1;}
        if end==start||end-start>9{return None;}
        for i in 0..3{milliseconds=milliseconds*10+if start+i<end{u16::from(bytes[start+i]-b'0')}else{0};}
    }
    let zone=&s[end..];let offset=if matches!(zone,"Z"|"z"){0_i64}else{
        let z=zone.as_bytes();if z.len()!=6||!matches!(z[0],b'+'|b'-')||z[3]!=b':'{return None;}
        if !z[1..3].iter().chain(&z[4..6]).all(u8::is_ascii_digit){return None;}
        let h=zone[1..3].parse::<i64>().ok()?;let m=zone[4..6].parse::<i64>().ok()?;
        if h>23||m>59{return None;}(h*3600+m*60)*if z[0]==b'+'{1}else{-1}
    };
    let time=SYSTEMTIME{wYear:year,wMonth:month,wDay:day,wHour:hour,wMinute:minute,wSecond:second,wMilliseconds:milliseconds,wDayOfWeek:0};
    let utc=i128::from(to_ticks(&time)?)-i128::from(offset)*10_000_000;
    from_ticks(u64::try_from(utc).ok()?)
}
/// Seconds since 1601 for an ISO timestamp, used to tell how far apart two messages are.
pub fn seconds(timestamp:&str)->Option<i64>{parse_utc(timestamp).and_then(|t|to_ticks(&t)).map(|ticks|(ticks/10_000_000)as i64)}
pub fn preview_timestamp(days_ago:u64)->String{
    let mut now=unsafe{std::mem::zeroed()};unsafe{GetSystemTime(&mut now)};
    let time=to_ticks(&now).and_then(|ticks|ticks.checked_sub(days_ago*864_000_000_000)).and_then(from_ticks).unwrap_or(now);
    format!("{:04}-{:02}-{:02}T{:02}:{:02}:{:02}Z",time.wYear,time.wMonth,time.wDay,time.wHour,time.wMinute,time.wSecond)
}

#[cfg(test)]mod tests{
    use super::*;
    #[test]fn same_day_is_time_only_and_prior_calendar_days_have_dates(){
        let local=|year,month,day,hour,minute|SYSTEMTIME{wYear:year,wMonth:month,wDay:day,wHour:hour,wMinute:minute,..unsafe{std::mem::zeroed()}};
        let day=[2026,10,8];assert_eq!(format_at(&local(2026,10,8,0,1),day),"00:01");assert_eq!(format_at(&local(2026,10,7,23,59),day),"23:59 · Oct 7, 2026");
        assert_eq!(format_at(&local(2025,12,31,16,21),day),"16:21 · Dec 31, 2025");
        let mut clock=Clock::default();assert_eq!(clock.label("4:21 PM"),"4:21 PM");
        let live=clock.label(&preview_timestamp(0));assert_eq!(live.len(),5,"current local-time conversion: {live}");assert_eq!(&live[2..3],":");
    }
    #[test]fn iso_offsets_fractions_leap_dates_and_local_midnight_are_handled(){
        let utc=parse_utc("2026-10-08T00:15:30.123456+02:00").unwrap();assert_eq!((utc.wDay,utc.wHour,utc.wMinute,utc.wMilliseconds),(7,22,15,123));
        let utc=parse_utc("2026-10-07T23:15:30-02:00").unwrap();assert_eq!((utc.wDay,utc.wHour),(8,1));
        let utc=parse_utc("2026-10-08T02:15:00Z").unwrap();
        let mut zone:windows_sys::Win32::System::Time::TIME_ZONE_INFORMATION=unsafe{std::mem::zeroed()};zone.Bias=240;
        let mut local=unsafe{std::mem::zeroed()};assert_ne!(unsafe{windows_sys::Win32::System::Time::SystemTimeToTzSpecificLocalTime(&zone,&utc,&mut local)},0);
        assert_eq!(format_at(&local,[2026,10,8]),"22:15 · Oct 7, 2026");
        assert!(parse_utc("2024-02-29T12:00:00Z").is_some());
        for bad in ["2026-02-29T12:00:00Z","2026-10-08T25:00:00Z","2026-10-08T12:00:00+24:00","2026-10-08T12:00:00.Z","2026-10-08T12:00:00","🙂2026-10-08T12:00:00Z"]{assert!(parse_utc(bad).is_none(),"{bad}");}
    }
}
