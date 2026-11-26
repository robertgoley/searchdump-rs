//! Remote storage URLs (s3://, b2://, sftp://, fs://) around a local run.

use crate::*;

/// L-809a: storage-URL directories. `copy` copies a dump directory between stores; a dump to
/// a storage URL is written to a local staging directory and uploaded when every index
/// succeeded; a load from one is downloaded to a staging directory first.
#[cfg_attr(not(feature = "opendal"), allow(unused_variables, unused_mut))]
pub(crate) async fn run(mut args: Args) -> Result<i32> {
    use searchdump_rs::remote;
    if args.parallel == 0 {
        bail!("--parallel must be at least 1");
    }
    let in_cfg = args.input_storage_config.clone().or_else(|| {
        searchdump_rs::envnames::var_os("SEARCHDUMP_INPUT_STORAGE_CONFIG").map(PathBuf::from)
    });
    let out_cfg = args.output_storage_config.clone().or_else(|| {
        searchdump_rs::envnames::var_os("SEARCHDUMP_OUTPUT_STORAGE_CONFIG").map(PathBuf::from)
    });
    let is_cluster = |u: &str| {
        let l = u.to_ascii_lowercase();
        l.starts_with("http://") || l.starts_with("https://")
    };
    match args.direction {
        Direction::Copy => {
            if is_cluster(&args.input) || is_cluster(&args.output) {
                bail!(
                    "--direction copy copies a dump directory between stores; use dump or load for a cluster"
                );
            }
            if ClusterTls::any_flag(&args) {
                bail!(
                    "--direction copy has no cluster, so --ca/--cert/--key/--pass do not apply; set ca_file= or insecure= in the storage config for an s3:// or b2:// endpoint"
                );
            }
            if args.node_compat
                || args.file_size.is_some()
                || args.max_rows.is_some()
                || args.page_bytes.is_some()
            {
                bail!(
                    "--direction copy copies the files as they are (format, compression and split parts kept); --nodeCompat, --fileSize, --maxRows and --pageBytes do not apply"
                );
            }
            #[cfg(feature = "opendal")]
            {
                let src = remote::parse_dir(&args.input, in_cfg.as_deref())?;
                let dst = remote::parse_dir(&args.output, out_cfg.as_deref())?;
                if src == dst {
                    bail!("--input and --output are the same directory");
                }
                let r = remote::copy_tree(&src, &dst, args.overwrite, args.parallel).await?;
                println!(
                    "copied {} files ({} bytes) from {src} to {dst}",
                    r.files, r.bytes
                );
                Ok(0)
            }
            #[cfg(not(feature = "opendal"))]
            bail!("--direction copy needs a build with the `opendal` feature");
        }
        Direction::Dump if remote::is_storage_url(&args.output) => {
            #[cfg(feature = "opendal")]
            {
                let dst = remote::parse_dir(&args.output, out_cfg.as_deref())?;
                let (staging, temp) = staging_dir(&args)?;
                args.output = staging.display().to_string();
                if args.meta_dir.is_some() {
                    bail!("--metaDir cannot be combined with a storage-URL --output");
                }
                if args.dry_run {
                    return run_local(args).await;
                }
                eprintln!(
                    "dumping to the staging directory {} first",
                    staging.display()
                );
                let code = run_local(args.clone()).await?;
                if code != 0 {
                    eprintln!(
                        "not uploaded to {dst}: some steps failed. The dump is kept in {}; finish it with --stagingDir {} --resume (same arguments), which uploads it",
                        staging.display(),
                        staging.display()
                    );
                    return Ok(code);
                }
                let src = remote::parse_dir(&staging.display().to_string(), None)?;
                let r = remote::copy_tree(&src, &dst, args.overwrite, args.parallel)
                    .await
                    .with_context(|| {
                        format!(
                            "uploading the dump failed; it is kept in {} (rerun with --stagingDir {} --resume, or copy it with --direction copy)",
                            staging.display(),
                            staging.display()
                        )
                    })?;
                println!("uploaded {} files ({} bytes) to {dst}", r.files, r.bytes);
                if temp {
                    let _ = fs::remove_dir_all(&staging);
                } else {
                    eprintln!("the staged copy stays in {}", staging.display());
                }
                Ok(0)
            }
            #[cfg(not(feature = "opendal"))]
            bail!("a storage-URL --output needs a build with the `opendal` feature");
        }
        Direction::Load if remote::is_storage_url(&args.input) => {
            #[cfg(feature = "opendal")]
            {
                let src = remote::parse_dir(&args.input, in_cfg.as_deref())?;
                let (staging, temp) = staging_dir(&args)?;
                if args.meta_dir.is_some() {
                    bail!("--metaDir cannot be combined with a storage-URL --input");
                }
                let dst = remote::parse_dir(&staging.display().to_string(), None)?;
                let r = remote::copy_tree(&src, &dst, true, args.parallel).await?;
                eprintln!(
                    "downloaded {} files ({} bytes) from {src} to {}",
                    r.files,
                    r.bytes,
                    staging.display()
                );
                args.input = staging.display().to_string();
                let code = run_local(args).await;
                if temp {
                    let _ = fs::remove_dir_all(&staging);
                }
                code
            }
            #[cfg(not(feature = "opendal"))]
            bail!("a storage-URL --input needs a build with the `opendal` feature");
        }
        Direction::Dump if remote::is_storage_url(&args.input) => bail!(
            "--direction dump reads a cluster; to copy a dump directory between stores use --direction copy"
        ),
        Direction::Load if remote::is_storage_url(&args.output) => bail!(
            "--direction load writes into a cluster; to copy a dump directory between stores use --direction copy"
        ),
        _ => run_local(args).await,
    }
}

/// `--stagingDir`, or a new temp directory (`true`: made here, removed when done).
#[cfg(feature = "opendal")]
pub(crate) fn staging_dir(args: &Args) -> Result<(PathBuf, bool)> {
    match &args.staging_dir {
        Some(d) => {
            fs::create_dir_all(d).with_context(|| format!("create {}", d.display()))?;
            Ok((d.clone(), false))
        }
        None => {
            let d = std::env::temp_dir().join(format!(
                "searchdump-multi-staging-{}-{}",
                std::process::id(),
                chrono_stamp()
            ));
            fs::create_dir_all(&d).with_context(|| format!("create {}", d.display()))?;
            Ok((d, true))
        }
    }
}

pub(crate) fn chrono_stamp() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}
