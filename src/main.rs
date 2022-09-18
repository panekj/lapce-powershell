use std::{
  env,
  fs::{self, File},
  io,
  path::PathBuf,
};

use anyhow::Result;
use lapce_plugin::{
  psp_types::{
    lsp_types::{request::Initialize, DocumentFilter, DocumentSelector, InitializeParams, Url},
    Request,
  },
  register_plugin, Http, LapcePlugin, VoltEnvironment, PLUGIN_RPC,
};
use serde_json::Value;

#[derive(Default)]
struct State {}

register_plugin!(State);

macro_rules! string {
  ( $x:expr ) => {
    String::from($x)
  };
}

const LSP_VERSION: &str = "3.5.1";
const PSES_DIRECTORY: &str = "PSES";

fn initialize(params: InitializeParams) -> Result<()> {
  PLUGIN_RPC.stderr("PowerShell Plugin starting");

  let workdir = Url::parse(&VoltEnvironment::uri()?)?
    .to_file_path()
    .unwrap();

  PLUGIN_RPC.stderr(&format!("{workdir:?}"));

  // let session_path = PathBuf::from(format!(
  //   "pses-{}",
  //   std::time::SystemTime::now()
  //     .duration_since(std::time::UNIX_EPOCH)
  //     .unwrap()
  //     .as_secs()
  // ));

  let session_path = PathBuf::new();

  PLUGIN_RPC.stderr(&format!("{session_path:?}"));

  let document_selector: DocumentSelector = vec![DocumentFilter {
    language: Some(String::from("powershell")),
    pattern: Some(String::from("**/*.ps1")),
    scheme: None,
  }];
  let mut server_args = vec![];

  if VoltEnvironment::operating_system().as_deref() == Ok("windows") {
    server_args.push(string!("-ExecutionPolicy"));
    server_args.push(string!("Bypass"));
  }

  server_args.append(&mut vec![
    string!("-NoLogo"),
    string!("-NoProfile"),
    string!("-NoExit"),
    string!("-Interactive"),
    string!("-Command"),
    format!(r#"'& "{}" -BundledModulesPath "{}" -LogPath "{}" -SessionDetailsPath "{}" -HostName "Lapce Host" -HostProfileId lapce -HostVersion 1.0.0 -Stdio -LogLevel Diagnostic'"#,
    workdir.join(PSES_DIRECTORY).join("PowerShellEditorServices").join("Start-EditorServices.ps1").display(),
    workdir.join(PSES_DIRECTORY).display(),
    workdir.join("logs.log").display(),
    workdir.join("session.json").display())
  ]);

  PLUGIN_RPC.stderr(&format!("args: {}", server_args.join(" ")));

  if let Some(options) = params.initialization_options.as_ref() {
    if let Some(lsp) = options.get("lsp") {
      if let Some(args) = lsp.get("serverArgs") {
        if let Some(args) = args.as_array() {
          if !args.is_empty() {
            server_args = vec![];
          }
          for arg in args {
            if let Some(arg) = arg.as_str() {
              server_args.push(arg.to_string());
            }
          }
        }
      }

      if let Some(server_path) = lsp.get("serverPath") {
        if let Some(server_path) = server_path.as_str() {
          if !server_path.is_empty() {
            let server_uri = Url::parse(&format!("urn:{}", server_path))?;
            PLUGIN_RPC.start_lsp(
              server_uri,
              server_args,
              document_selector,
              params.initialization_options,
            );

            PLUGIN_RPC.stderr("LSP started");

            return Ok(());
          }
        }
      }
    }
  }

  let zip_file = string!("PowerShellEditorServices.zip");

  let download_url = format!(
    "https://github.com/PowerShell/PowerShellEditorServices/releases/download/v{LSP_VERSION}/{}",
    zip_file
  );

  let zip_file = PathBuf::from(zip_file);

  if !PathBuf::from(PSES_DIRECTORY)
    .join("PowerShellEditorServices")
    .join("Start-EditorServices.ps1")
    .exists()
  {
    if zip_file.exists() {
      fs::remove_file(&zip_file)?;
    }
    let mut resp = Http::get(&download_url)?;
    PLUGIN_RPC.stderr(&format!("STATUS_CODE: {:?}", resp.status_code));
    let body = resp.body_read_all()?;
    fs::write(&zip_file, body)?;

    let mut zip = zip::ZipArchive::new(File::open(&zip_file)?)?;

    fs::create_dir(PSES_DIRECTORY)?;
    env::set_current_dir(PSES_DIRECTORY)?;

    for i in 0..zip.len() {
      let mut file = zip.by_index(i)?;
      let outpath = match file.enclosed_name() {
        | Some(path) => path.to_owned(),
        | None => continue,
      };

      if (*file.name()).ends_with('/') {
        fs::create_dir_all(&outpath)?;
      } else {
        if let Some(p) = outpath.parent() {
          if !p.exists() {
            fs::create_dir_all(&p)?;
          }
        }
        let mut outfile = File::create(&outpath)?;
        io::copy(&mut file, &mut outfile)?;
      }
    }

    fs::remove_file(&zip_file)?;
  }

  let server_uri = if VoltEnvironment::operating_system().as_deref() == Ok("windows") {
    Url::parse("urn:powershell.exe")?
  } else {
    Url::parse("urn:pwsh")?
  };

  PLUGIN_RPC.start_lsp(
    server_uri,
    server_args,
    document_selector,
    params.initialization_options,
  );

  Ok(())
}

impl LapcePlugin for State {
  fn handle_request(&mut self, _id: u64, method: String, params: Value) {
    #[allow(clippy::single_match)]
    match method.as_str() {
      | Initialize::METHOD => {
        let params: InitializeParams = serde_json::from_value(params).unwrap();
        match initialize(params) {
          | Ok(_) => PLUGIN_RPC.stderr("no error"),
          | Err(e) => PLUGIN_RPC.stderr(&format!("error: {e}")),
        };
      }
      | _ => {}
    }
  }
}
