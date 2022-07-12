mod utils;

use wasm_bindgen::JsCast;
use wasm_bindgen::prelude::*;
use minidump::*;
use std::io::{self, Write};
use breakpad_symbols::{Module, SymbolFile, SymbolResult, SymbolSupplier, relative_symbol_path, Symbolizer};
use wasm_bindgen_futures::JsFuture;
use web_sys::{Request, RequestInit, RequestMode, Response, RequestCache};
use minidump_processor::{MultiSymbolProvider};

use async_trait::async_trait;

// When the `wee_alloc` feature is enabled, use `wee_alloc` as the global
// allocator.
#[cfg(feature = "wee_alloc")]
#[global_allocator]
static ALLOC: wee_alloc::WeeAlloc = wee_alloc::WeeAlloc::INIT;

#[wasm_bindgen]
extern "C" {
    // Use `js_namespace` here to bind `console.log(..)` instead of just
    // `log(..)`
    #[wasm_bindgen(js_namespace = console)]
    fn log(s: &str);
}


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

/// An implementation of `SymbolSupplier` that loads Breakpad text-format symbols from HTTP
/// URLs over fetch().
pub struct FetchSymbolSupplier {
    /// URLs to search for symbols.
    urls: Vec<String>,
}

impl FetchSymbolSupplier {
    pub fn new(
        urls: Vec<String>,
    ) -> FetchSymbolSupplier {
        let urls = urls
            .into_iter()
            .map(|mut u| {
                if !u.ends_with('/') {
                    u.push('/');
                }
                u
            })
            .collect();
        FetchSymbolSupplier {
            urls,
        }
    }
}

/// Fetch a symbol file from the URL made by combining `base_url` and `rel_path` using `client`,
/// save the file contents under `cache` + `rel_path` and also return them.
async fn fetch_symbol_file(
    base_url: &str,
    rel_path: &str,
) -> Result<Vec<u8>, Error> {
    let url = base_url.to_owned() + rel_path;
    let mut opts = RequestInit::new();
    opts.method("GET");
    opts.mode(RequestMode::Cors);
    opts.cache(RequestCache::ForceCache);

    let request = Request::new_with_str_and_init(&url, &opts).unwrap();

    let window = web_sys::window().unwrap();
    let resp_value = JsFuture::from(window.fetch_with_request(&request)).await.map_err(|_| Error::IoError)?;

    assert!(resp_value.is_instance_of::<Response>());
    let resp: Response = resp_value.dyn_into().unwrap();

    let buf = JsFuture::from(resp.array_buffer().unwrap()).await.unwrap();

    let typebuf = js_sys::Uint8Array::new(&buf);
    let mut body = vec![0; typebuf.length() as usize];
    typebuf.copy_to(&mut body[..]);
    Ok(body.to_vec())
}

#[async_trait(?Send)]
impl SymbolSupplier for FetchSymbolSupplier {
    async fn locate_symbols(&self, module: &dyn Module) -> SymbolResult {
        log("here");
        // Check local paths first.
        if let Some(rel_path) = relative_symbol_path(module, "sym") {
            for ref url in self.urls.iter() {
                if let Ok(buf) =
                    fetch_symbol_file(url, &rel_path).await
                {
                    return SymbolFile::from_bytes(&buf)
                        .map(SymbolResult::Ok)
                        .unwrap_or_else(SymbolResult::LoadError);
                }
            }
        }
        SymbolResult::NotFound
    }
}

#[wasm_bindgen]
pub async fn parse(data: Vec<u8>) -> Result<JsValue, JsValue> {
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

            let mut provider = MultiSymbolProvider::new();
            provider.add(Box::new(Symbolizer::new(FetchSymbolSupplier::new(
                vec!["https://symbols.electronjs.org".to_string(), "https://symbols.mozilla.org/try".to_string()]
            ))));
            match minidump_processor::process_minidump(&dump, &provider).await {
                Ok(state) => {
                    let human = true;
                    let pretty = true;
                    if human {
                        state.print(&mut stdout).unwrap();
                    } else {
                        state.print_json(&mut stdout, pretty).unwrap();
                    }
                }
                Err(err) => {
                    eprintln!("Error processing dump: {:?}", err);
                }
            }
            Ok(JsValue::from_str(std::str::from_utf8(&stdout).unwrap()))
        }
        Err(err) => {
            panic!("it did not work: {:?}", err)
        }
    }
}
