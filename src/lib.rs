mod utils;

use wasm_bindgen::prelude::*;
use minidump::*;
use std::io::{self, Write};

// When the `wee_alloc` feature is enabled, use `wee_alloc` as the global
// allocator.
#[cfg(feature = "wee_alloc")]
#[global_allocator]
static ALLOC: wee_alloc::WeeAlloc = wee_alloc::WeeAlloc::INIT;

macro_rules! streams {
    ( $( $x:ident ),* ) => {
        &[$( ( minidump_common::format::MINIDUMP_STREAM_TYPE::$x, stringify!($x) ) ),*]
    };
}

fn print_raw_stream<T: Write>(name: &str, contents: &[u8], out: &mut T) -> io::Result<()> {
    writeln!(out, "Stream {}:", name)?;
    let s = contents
        .split(|&v| v == 0)
        .map(|s| String::from_utf8_lossy(s))
        .collect::<Vec<_>>()
        .join("\\0\n");
    write!(out, "{}\n\n", s)
}

#[wasm_bindgen]
extern {
    fn alert(s: &str);
}

#[wasm_bindgen]
pub fn parse(data: &[u8]) -> Result<JsValue, JsValue> {
    match Minidump::read(data) {
        Ok(dump) => {
            let mut out: Vec<u8> = Vec::new();
            let mut stdout = &mut out;
            dump.print(stdout);
            if let Ok(thread_list) = dump.get_stream::<MinidumpThreadList<'_>>() {
                thread_list.print(stdout).unwrap();
            }
            if let Ok(module_list) = dump.get_stream::<MinidumpModuleList>() {
                module_list.print(stdout).unwrap();
            }
            if let Ok(memory_list) = dump.get_stream::<MinidumpMemoryList<'_>>() {
                memory_list.print(stdout).unwrap();
            }
            // TODO: MemoryList
            if let Ok(exception) = dump.get_stream::<MinidumpException>() {
                exception.print(stdout).unwrap();
            }
            if let Ok(assertion) = dump.get_stream::<MinidumpAssertion>() {
                assertion.print(stdout).unwrap();
            }
            if let Ok(system_info) = dump.get_stream::<MinidumpSystemInfo>() {
                system_info.print(stdout).unwrap();
            }
            if let Ok(misc_info) = dump.get_stream::<MinidumpMiscInfo>() {
                misc_info.print(stdout).unwrap();
            }
            if let Ok(breakpad_info) = dump.get_stream::<MinidumpBreakpadInfo>() {
                breakpad_info.print(stdout).unwrap();
            }
            // TODO: MemoryInfoList
            match dump.get_stream::<MinidumpCrashpadInfo>() {
                Ok(crashpad_info) => crashpad_info.print(stdout).unwrap(),
                Err(Error::StreamNotFound) => (),
                Err(_) => write!(stdout, "MinidumpCrashpadInfo cannot print invalid data").unwrap(),
            }
            for &(stream, name) in streams!(
                LinuxCmdLine,
                LinuxEnviron,
                LinuxLsbRelease,
                LinuxProcStatus,
                LinuxCpuInfo,
                LinuxMaps
            ) {
                if let Ok(contents) = dump.get_raw_stream(stream) {
                    print_raw_stream(name, contents, stdout).unwrap();
                }
            }
            Ok(JsValue::from_str(std::str::from_utf8(&stdout).unwrap()))
        }
        Err(err) => {
            panic!("it did not work: {:?}", err)
        }
    }
}
