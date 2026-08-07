use std::{
    env, fs,
    io::{self, BufRead, Write},
    path::{Path, PathBuf},
    process,
};

fn main() {
    if let Err(error) = run() {
        eprintln!("runtime fixture failed: {error}");
        process::exit(29);
    }
}

fn run() -> Result<(), Box<dyn std::error::Error>> {
    let executable = env::current_exe()?;
    let tool = executable
        .file_stem()
        .and_then(|name| name.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();
    let arguments = env::args_os().skip(1).collect::<Vec<_>>();

    if arguments.first().and_then(|value| value.to_str()) == Some("--version") {
        let version = fs::read_to_string(executable.with_extension("version"))?;
        println!("{tool} {}", version.trim());
        return Ok(());
    }

    if tool.contains("codex") {
        return run_codex(&executable);
    }
    if tool == "uv" {
        return run_uv(&executable, &arguments);
    }
    if tool == "python" {
        return run_prepare_models(&arguments);
    }
    if tool.contains("docling") {
        return run_docling(&arguments);
    }
    Err(format!("unrecognized fixture tool {tool}").into())
}

fn run_codex(executable: &Path) -> Result<(), Box<dyn std::error::Error>> {
    let mut requests = io::BufReader::new(io::stdin()).lines();
    let initialize = requests.next().ok_or("missing initialize request")??;
    if !initialize.contains("\"method\":\"initialize\"")
        || initialize.contains("\"method\":\"account/read\"")
    {
        return Err("invalid initialize sequence".into());
    }
    println!(
        "{}",
        serde_json::json!({
            "id": 0,
            "result": {
                "codexHome": executable.parent().ok_or("missing fixture parent")?,
                "userAgent": "fixture",
                "platformFamily": if cfg!(windows) { "windows" } else { "unix" },
                "platformOs": env::consts::OS,
            }
        })
    );
    io::stdout().flush()?;
    let initialized = requests
        .next()
        .ok_or("missing initialized notification")??;
    let account_read = requests.next().ok_or("missing account/read request")??;
    if !initialized.contains("\"method\":\"initialized\"")
        || !account_read.contains("\"method\":\"account/read\"")
    {
        return Err("invalid post-initialize sequence".into());
    }
    let probe_delay = executable.with_extension("probe-delay");
    if probe_delay.is_file() {
        fs::write(executable.with_extension("probe-ready"), b"ready")?;
        let milliseconds = fs::read_to_string(probe_delay)?.trim().parse()?;
        std::thread::sleep(std::time::Duration::from_millis(milliseconds));
    }
    match fs::read_to_string(executable.with_extension("auth"))?.trim() {
        "chatgpt" => println!(
            "{}",
            serde_json::json!({
                "id": 1,
                "result": {
                    "account": {
                        "type": "chatgpt",
                        "email": null,
                        "planType": fs::read_to_string(executable.with_extension("plan"))?.trim(),
                    },
                    "requiresOpenaiAuth": true,
                }
            })
        ),
        "none" => println!(
            "{{\"id\":1,\"result\":{{\"account\":null,\"requiresOpenaiAuth\":true}}}}"
        ),
        "apikey" => println!(
            "{{\"id\":1,\"result\":{{\"account\":{{\"type\":\"apiKey\"}},\"requiresOpenaiAuth\":true}}}}"
        ),
        "malformed" => println!("not-json"),
        "mixed" => {
            println!("not-json");
            println!(
                "{{\"id\":1,\"result\":{{\"account\":{{\"type\":\"chatgpt\",\"email\":null,\"planType\":\"plus\"}},\"requiresOpenaiAuth\":true}}}}"
            );
        }
        state => return Err(format!("unknown auth fixture state {state}").into()),
    }
    Ok(())
}

fn run_uv(
    executable: &Path,
    arguments: &[std::ffi::OsString],
) -> Result<(), Box<dyn std::error::Error>> {
    assert_isolated_python_environment()?;
    if arguments.first().and_then(|value| value.to_str()) != Some("sync") {
        return Err("unexpected uv fixture command".into());
    }
    for required in [
        "--locked",
        "--no-dev",
        "--managed-python",
        "--python",
        "--project",
        "--no-config",
    ] {
        if !arguments.iter().any(|argument| argument == required) {
            return Err(format!("missing required uv argument {required}").into());
        }
    }
    let delay = executable.with_extension("delay");
    if delay.is_file() {
        let milliseconds = fs::read_to_string(delay)?.trim().parse()?;
        std::thread::sleep(std::time::Duration::from_millis(milliseconds));
    }
    let environment =
        PathBuf::from(env::var_os("UV_PROJECT_ENVIRONMENT").ok_or("missing project environment")?);
    let python =
        PathBuf::from(env::var_os("UV_PYTHON_INSTALL_DIR").ok_or("missing Python install root")?);
    let python_downloads = PathBuf::from(
        env::var_os("UV_PYTHON_DOWNLOADS_JSON_URL")
            .ok_or("missing approved Python downloads manifest")?,
    );
    if !python_downloads.is_file() {
        return Err("approved Python downloads manifest is missing".into());
    }
    if arguments.iter().any(|argument| argument == "--check") {
        if !arguments.iter().any(|argument| argument == "--offline")
            || !environment.join("fixture_dependency.py").is_file()
            || !python.join("managed-python.txt").is_file()
        {
            return Err("managed environment is not synchronized".into());
        }
        return Ok(());
    }
    let binary_directory = if cfg!(windows) {
        environment.join("Scripts")
    } else {
        environment.join("bin")
    };
    fs::create_dir_all(&binary_directory)?;
    fs::create_dir_all(&python)?;
    fs::write(environment.join("fixture_dependency.py"), b"fixture=true\n")?;
    fs::write(python.join("managed-python.txt"), b"3.12.13\n")?;
    let concrete_python = python.join(format!(
        "cpython-3.12.13-{}-{}-none",
        env::consts::OS,
        env::consts::ARCH
    ));
    fs::create_dir_all(&concrete_python)?;
    fs::write(concrete_python.join("install.txt"), b"3.12.13\n")?;
    let minor_python = python.join(format!(
        "cpython-3.12-{}-{}-none",
        env::consts::OS,
        env::consts::ARCH
    ));
    #[cfg(windows)]
    junction::create(&concrete_python, &minor_python)?;
    #[cfg(unix)]
    std::os::unix::fs::symlink(&concrete_python, &minor_python)?;
    #[cfg(unix)]
    {
        let library = environment.join("lib");
        fs::create_dir_all(&library)?;
        std::os::unix::fs::symlink(&library, environment.join("lib64"))?;
    }
    let extension = env::consts::EXE_EXTENSION;
    for name in ["docling", "python"] {
        let destination = binary_directory.join(if extension.is_empty() {
            name.to_owned()
        } else {
            format!("{name}.{extension}")
        });
        fs::copy(executable, &destination)?;
        fs::write(
            destination.with_extension("version"),
            fs::read_to_string(executable.with_file_name("docling.version"))?,
        )?;
        make_executable(&destination)?;
    }
    Ok(())
}

fn run_prepare_models(arguments: &[std::ffi::OsString]) -> Result<(), Box<dyn std::error::Error>> {
    assert_isolated_python_environment()?;
    let script = arguments
        .first()
        .ok_or("missing model preparation script")?;
    if Path::new(script).file_name().and_then(|name| name.to_str()) != Some("prepare_models.py") {
        return Err("unexpected model preparation script".into());
    }
    let output = argument_value(arguments, "--output-dir")?;
    for profile in [
        "layout",
        "tableformer",
        "code_formula",
        "picture_classifier",
        "rapidocr",
    ] {
        let model = output.join(profile).join("model.bin");
        fs::create_dir_all(model.parent().ok_or("model parent")?)?;
        fs::write(model, format!("{profile} fixture model"))?;
    }
    println!("{}", output.display());
    Ok(())
}

fn run_docling(arguments: &[std::ffi::OsString]) -> Result<(), Box<dyn std::error::Error>> {
    assert_isolated_python_environment()?;
    if arguments.first().and_then(|value| value.to_str()) != Some("convert") {
        return Err("unexpected Docling fixture command".into());
    }
    let output = argument_value(arguments, "--output")?;
    let models = argument_value(arguments, "--artifacts-path")?;
    for profile in [
        "layout",
        "tableformer",
        "code_formula",
        "picture_classifier",
        "rapidocr",
    ] {
        if !models.join(profile).join("model.bin").is_file() {
            return Err(format!("fixture {profile} model is missing").into());
        }
    }
    if argument_value(arguments, "--ocr-engine")? != Path::new("rapidocr")
        || argument_value(arguments, "--ocr-mode")? != Path::new("full_page")
        || argument_value(arguments, "--ocr-lang")? != Path::new("ch")
    {
        return Err("RapidOCR full-page smoke was not requested".into());
    }
    fs::create_dir_all(&output)?;
    fs::write(
        output.join("readiness.json"),
        serde_json::to_vec(&serde_json::json!({
            "schema_name": "DoclingDocument",
            "name": "readiness",
            "origin": { "mimetype": "application/pdf" },
            "texts": [{
                "text": "Docling bundles PDF document conversion to JSON and Markdown in an easy self contained package"
            }],
            "pages": { "1": { "size": { "width": 1, "height": 1 } } },
        }))?,
    )?;
    Ok(())
}

fn assert_isolated_python_environment() -> Result<(), Box<dyn std::error::Error>> {
    for name in ["PYTHONPATH", "PYTHONHOME", "VIRTUAL_ENV", "CONDA_PREFIX"] {
        if env::var_os(name).is_some() {
            return Err(format!("uncontrolled environment variable was inherited: {name}").into());
        }
    }
    Ok(())
}

fn argument_value(
    arguments: &[std::ffi::OsString],
    name: &str,
) -> Result<PathBuf, Box<dyn std::error::Error>> {
    let index = arguments
        .iter()
        .position(|value| value == name)
        .ok_or_else(|| format!("missing {name}"))?;
    arguments
        .get(index + 1)
        .map(PathBuf::from)
        .ok_or_else(|| format!("missing {name} value").into())
}

#[cfg(unix)]
fn make_executable(path: &Path) -> io::Result<()> {
    use std::os::unix::fs::PermissionsExt;

    let mut permissions = fs::metadata(path)?.permissions();
    permissions.set_mode(0o700);
    fs::set_permissions(path, permissions)
}

#[cfg(windows)]
fn make_executable(_path: &Path) -> io::Result<()> {
    Ok(())
}
