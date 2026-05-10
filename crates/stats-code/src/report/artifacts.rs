use super::{
    extract_string_field, fingerprint_file, fs, json, resolve_path_str_for_match, stringify_error,
    unix_timestamp_nanos, ArtifactMetadata, Path, PathBuf, RunArtifactContext, Value,
    ARTIFACT_SCHEMA_VERSION,
};

pub(crate) fn persist_run_artifacts_with_metadata(
    base_dir: &Path,
    command_name: &str,
    request: &Value,
    response: &Value,
    artifact: Option<&ArtifactMetadata>,
) -> Result<PathBuf, String> {
    let run_dir = base_dir.join(format!("{}-{}", command_name, unix_timestamp_nanos()));
    let artifact_id = run_dir
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or(command_name)
        .to_string();
    fs::create_dir_all(&run_dir).map_err(stringify_error)?;
    fs::write(
        run_dir.join("command.json"),
        serde_json::to_string_pretty(&json!({
            "artifact_schema_version": ARTIFACT_SCHEMA_VERSION,
            "artifact_id": &artifact_id,
            "stats_code_version": env!("CARGO_PKG_VERSION"),
            "command": command_name,
            "request": request,
            "artifact": artifact,
        }))
        .map_err(stringify_error)?,
    )
    .map_err(stringify_error)?;
    fs::write(
        run_dir.join("result.json"),
        serde_json::to_string_pretty(response).map_err(stringify_error)?,
    )
    .map_err(stringify_error)?;
    fs::write(
        run_dir.join("context.json"),
        serde_json::to_string_pretty(&build_run_artifact_context(
            command_name,
            request,
            response,
            &artifact_id,
            artifact.cloned(),
        ))
        .map_err(stringify_error)?,
    )
    .map_err(stringify_error)?;
    Ok(run_dir)
}

fn build_run_artifact_context(
    command_name: &str,
    request: &Value,
    response: &Value,
    artifact_id: &str,
    artifact: Option<ArtifactMetadata>,
) -> RunArtifactContext {
    let cwd = std::env::current_dir().ok();
    let analysis_path = extract_string_field(response, &["analysis_path"])
        .or_else(|| extract_string_field(request, &["analysis"]));
    let data_path = extract_string_field(response, &["data_path"])
        .or_else(|| extract_string_field(request, &["data", "data_path"]));

    RunArtifactContext {
        artifact_schema_version: Some(ARTIFACT_SCHEMA_VERSION.to_string()),
        artifact_id: Some(artifact_id.to_string()),
        stats_code_version: Some(env!("CARGO_PKG_VERSION").to_string()),
        command: command_name.to_string(),
        analysis_path: analysis_path.clone(),
        analysis_path_resolved: analysis_path
            .as_deref()
            .map(|path| resolve_path_str_for_match(path, cwd.as_deref())),
        analysis_fingerprint_fnv1a64: analysis_path.as_deref().and_then(|path| {
            let resolved = resolve_path_str_for_match(path, cwd.as_deref());
            fingerprint_file(Path::new(&resolved))
        }),
        data_path: data_path.clone(),
        data_path_resolved: data_path
            .as_deref()
            .map(|path| resolve_path_str_for_match(path, cwd.as_deref())),
        data_fingerprint_fnv1a64: data_path.as_deref().and_then(|path| {
            let resolved = resolve_path_str_for_match(path, cwd.as_deref());
            fingerprint_file(Path::new(&resolved))
        }),
        cwd: cwd.map(|path| path.display().to_string()),
        generated_at_unix_nanos: Some(unix_timestamp_nanos()),
        artifact,
    }
}

// ---------------------------------------------------------------------------
// Artifact matching
// ---------------------------------------------------------------------------
