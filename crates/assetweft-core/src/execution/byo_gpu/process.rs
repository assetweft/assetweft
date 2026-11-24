use std::{
    env,
    fs::{self, File},
    io::{self, Read, Write},
    path::{Path, PathBuf},
    process::{Command, Stdio},
};

use sha2::{Digest, Sha256};

use super::{Invalid, KaggleValidation, LEDGER_SOURCE, Run, RunError, policy};
use crate::budget::{Request, Unit};

const METADATA: &str = "kernel-metadata.json";

struct Kernel {
    metadata: Vec<u8>,
    code: Vec<u8>,
    code_file: String,
    accelerator: &'static str,
}

pub(crate) struct Prepared<'a> {
    run: &'a Run,
    adapter: PathBuf,
    out_dir: PathBuf,
    image: Option<(PathBuf, Vec<u8>)>,
    kernel: Option<Kernel>,
    pub(crate) artifact: &'static str,
}

fn digest(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

fn read_regular(path: &Path, max: u64, field: &'static str) -> Result<Vec<u8>, Invalid> {
    let error = || {
        Invalid(
            field,
            "file must be readable, regular, nonempty and within the byte limit",
        )
    };
    let metadata = fs::symlink_metadata(path).map_err(|_| error())?;
    if !metadata.is_file() || metadata.len() == 0 || metadata.len() > max {
        return Err(error());
    }
    let mut bytes = Vec::new();
    File::open(path)
        .and_then(|file| file.take(max + 1).read_to_end(&mut bytes))
        .map_err(|_| error())?;
    if bytes.is_empty() || bytes.len() as u64 > max {
        return Err(error());
    }
    Ok(bytes)
}

fn read_kernel(bundle: &Path, max: u64) -> Result<Kernel, Invalid> {
    let invalid_dir = || Invalid("bundle", "bundle must be a readable, non-symlink directory");
    if !fs::symlink_metadata(bundle).is_ok_and(|m| m.is_dir()) {
        return Err(invalid_dir());
    }
    let root = bundle.canonicalize().map_err(|_| invalid_dir())?;
    let metadata = read_regular(&root.join(METADATA), max.min(65_536), "bundle")?;
    let (code_file, accelerator) = policy::kernel_metadata(&metadata)?;
    let mut code_path = root.clone();
    let invalid_code = || {
        Invalid(
            "bundle.code_file",
            "kernel code must stay within the bundle without symlinks",
        )
    };
    for component in code_file.split('/') {
        code_path.push(component);
        if fs::symlink_metadata(&code_path)
            .map_err(|_| invalid_code())?
            .is_symlink()
        {
            return Err(invalid_code());
        }
    }
    let absolute = code_path.canonicalize().map_err(|_| invalid_code())?;
    if !absolute.starts_with(&root) {
        return Err(invalid_code());
    }
    let code = read_regular(&code_path, max, "bundle.code_file")?;
    Ok(Kernel {
        metadata,
        code,
        code_file,
        accelerator,
    })
}

fn kaggle_credentials_present() -> bool {
    let present = |name| env::var_os(name).is_some_and(|v| !v.is_empty());
    present("KAGGLE_API_TOKEN")
        || (present("KAGGLE_USERNAME") && present("KAGGLE_KEY"))
        || env::var_os("KAGGLE_CONFIG_DIR")
            .filter(|path| !path.is_empty())
            .is_some_and(|path| Path::new(&path).join("kaggle.json").is_file())
        || env::home_dir().is_some_and(|home| {
            [".kaggle/kaggle.json", ".config/kaggle/kaggle.json"]
                .into_iter()
                .any(|file| home.join(file).is_file())
        })
}

pub fn validate_kaggle_bundle(bundle: &Path, max_bytes: u64) -> Result<KaggleValidation, Invalid> {
    if !(1..=16_777_216).contains(&max_bytes) {
        return Err(Invalid("max-bytes", "byte limit must be in 1..=16777216"));
    }
    let kernel = read_kernel(bundle, max_bytes)?;
    Ok(KaggleValidation {
        credential_present: kaggle_credentials_present(),
        accelerator: kernel.accelerator,
    })
}

fn executable(path: &Path) -> Option<PathBuf> {
    let absolute = path.canonicalize().ok()?;
    let metadata = absolute.metadata().ok()?;
    if !metadata.is_file() {
        return None;
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if metadata.permissions().mode() & 0o111 == 0 {
            return None;
        }
    }
    #[cfg(windows)]
    if !absolute
        .extension()
        .is_some_and(|ext| ext.eq_ignore_ascii_case("exe"))
    {
        return None;
    }
    Some(absolute)
}

fn adapter_path(explicit: Option<&Path>) -> Result<PathBuf, Invalid> {
    let configured = env::var_os("ASSETWEFT_BYO_GPU_ADAPTER");
    let path = explicit
        .or(configured.as_deref().map(Path::new))
        .filter(|p| !p.as_os_str().is_empty())
        .ok_or(Invalid(
            "adapter-bin",
            "set --adapter-bin or ASSETWEFT_BYO_GPU_ADAPTER",
        ))?;
    let resolved = if path.components().count() == 1 && path.file_name().is_some() {
        env::var_os("PATH").and_then(|search| {
            env::split_paths(&search)
                .filter(|dir| dir.is_absolute())
                .find_map(|dir| {
                    let candidate = dir.join(path);
                    let resolved = executable(&candidate);
                    #[cfg(windows)]
                    let resolved = resolved.or_else(|| {
                        candidate
                            .extension()
                            .is_none()
                            .then(|| executable(&candidate.with_extension("exe")))
                            .flatten()
                    });
                    resolved
                })
        })
    } else {
        executable(path)
    };
    resolved.ok_or(Invalid(
        "adapter-bin",
        "runner must resolve to a local executable",
    ))
}

fn output_dir(path: &Path) -> Result<PathBuf, Invalid> {
    let invalid = || {
        Invalid(
            "out-dir",
            "choose a directory or a new directory under an existing parent",
        )
    };
    match fs::symlink_metadata(path) {
        Ok(meta) if meta.is_dir() => path.canonicalize().map_err(|_| invalid()),
        Err(err) if err.kind() == io::ErrorKind::NotFound => {
            let name = path.file_name().ok_or_else(invalid)?;
            let parent = path
                .parent()
                .filter(|p| !p.as_os_str().is_empty())
                .unwrap_or(Path::new("."));
            let parent = parent.canonicalize().map_err(|_| invalid())?;
            if !parent.is_dir() {
                return Err(invalid());
            }
            Ok(parent.join(name))
        }
        _ => Err(invalid()),
    }
}

pub(crate) fn prepare(run: &Run) -> Result<Prepared<'_>, RunError> {
    let model = policy::check_run(run).map_err(RunError::Invalid)?;
    let kernel = run
        .bundle
        .as_deref()
        .map(|dir| read_kernel(dir, run.max_bytes))
        .transpose()
        .map_err(RunError::Invalid)?;
    if kernel.is_some() && !kaggle_credentials_present() {
        return Err(RunError::Credentials);
    }
    let image = run
        .image
        .as_deref()
        .map(|path| {
            let file = path
                .file_name()
                .ok_or(Invalid("image", "image file name required"))?;
            read_regular(path, run.max_bytes, "image").map(|bytes| (PathBuf::from(file), bytes))
        })
        .transpose()
        .map_err(RunError::Invalid)?;
    Ok(Prepared {
        run,
        adapter: adapter_path(run.adapter_bin.as_deref()).map_err(RunError::Invalid)?,
        out_dir: output_dir(&run.out_dir).map_err(RunError::Invalid)?,
        image,
        kernel,
        artifact: model.artifact,
    })
}

impl Prepared<'_> {
    pub(crate) fn request(&self) -> Request {
        let identity = (
            LEDGER_SOURCE,
            self.run.profile.id(),
            &self.run.model,
            self.adapter.as_os_str().as_encoded_bytes(),
            self.out_dir.as_os_str().as_encoded_bytes(),
            self.run.max_bytes,
            &self.run.prompt,
            self.image
                .as_ref()
                .map(|(name, bytes)| (name.as_os_str().as_encoded_bytes(), digest(bytes))),
            self.kernel
                .as_ref()
                .map(|k| (digest(&k.metadata), digest(&k.code))),
        );
        let encoded = serde_json::to_vec(&identity).expect("infallible JSON descriptor");
        Request {
            id: self.run.intent_id.clone(),
            account: self.run.budget_id.clone(),
            input_fingerprint: digest(&encoded),
            unit: Unit::Task,
            quote: None,
            reservation: 1,
            source: LEDGER_SOURCE.into(),
        }
    }

    pub(crate) fn check_destination(&self) -> Result<(), Invalid> {
        match fs::symlink_metadata(self.out_dir.join(self.artifact)) {
            Err(err) if err.kind() == io::ErrorKind::NotFound => Ok(()),
            _ => Err(Invalid(
                "out-dir",
                "artifact destination already exists or is inaccessible",
            )),
        }
    }

    pub(crate) fn invoke(&self) -> Result<String, &'static str> {
        let stage_error = |_| "could not stage the adapter inputs or output";
        if !self.out_dir.is_dir() {
            fs::create_dir(&self.out_dir).map_err(stage_error)?;
        }
        let work = tempfile::tempdir_in(&self.out_dir).map_err(stage_error)?;
        let output = work.path().join("output");
        fs::create_dir(&output).map_err(stage_error)?;
        let mut command = Command::new(&self.adapter);
        command
            .args([
                "--profile",
                self.run.profile.id(),
                "--model",
                &self.run.model,
                "--out-dir",
            ])
            .arg(&output)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        if let Some((name, bytes)) = &self.image {
            let directory = work.path().join("image");
            fs::create_dir(&directory).map_err(stage_error)?;
            let image = directory.join(name);
            fs::write(&image, bytes).map_err(stage_error)?;
            command.arg("--image").arg(image);
        }
        if let Some(prompt) = &self.run.prompt {
            command.arg(format!("--prompt={prompt}"));
        }
        if let Some(kernel) = &self.kernel {
            let bundle = work.path().join("bundle");
            let code_file = bundle.join(&kernel.code_file);
            fs::create_dir_all(code_file.parent().expect("relative kernel code"))
                .map_err(stage_error)?;
            fs::write(bundle.join(METADATA), &kernel.metadata).map_err(stage_error)?;
            fs::write(code_file, &kernel.code).map_err(stage_error)?;
            command
                .arg("--bundle")
                .arg(bundle)
                .args(["--accelerator", kernel.accelerator]);
        }
        let status = command
            .status()
            .map_err(|_| "could not start or wait for the external adapter")?;
        if !status.success() {
            return Err("external adapter reported failure");
        }
        let bytes = read_regular(&output.join(self.artifact), self.run.max_bytes, "artifact")
            .map_err(
                |_| "adapter did not produce the expected regular artifact within the byte limit",
            )?;
        let mut file = tempfile::NamedTempFile::new_in(&self.out_dir).map_err(stage_error)?;
        file.write_all(&bytes)
            .and_then(|()| file.as_file().sync_all())
            .map_err(stage_error)?;
        file.persist_noclobber(self.out_dir.join(self.artifact))
            .map_err(|_| "could not publish the new artifact without overwriting")?;
        Ok(digest(&bytes))
    }
}
