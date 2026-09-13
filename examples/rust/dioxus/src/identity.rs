//! The showcase owns storage; the SDK only loads or restores key material.

use wf_observer_sdk::ObserverIdentity;

#[cfg(not(all(target_family = "wasm", target_os = "unknown")))]
pub fn load() -> Result<ObserverIdentity, String> {
    let project = directories::ProjectDirs::from("", "", "wf-observer-showcase")
        .ok_or("application directories are unavailable")?;
    let path = project.config_local_dir().join("identity.key");
    wf_observer_sdk::load_identity(
        path.to_str()
            .ok_or("identity path is not UTF-8")?
            .to_owned(),
    )
    .map_err(|error| error.to_string())
}

#[cfg(all(target_family = "wasm", target_os = "unknown"))]
pub fn load() -> Result<ObserverIdentity, String> {
    const KEY: &str = "wf-observer-showcase.reader-identity.v1";
    let storage = web_sys::window()
        .ok_or("browser window is unavailable")?
        .local_storage()
        .map_err(|error| format!("cannot open browser storage: {error:?}"))?
        .ok_or("browser storage is unavailable")?;
    if let Some(secret) = storage
        .get_item(KEY)
        .map_err(|error| format!("cannot read identity: {error:?}"))?
    {
        let secret = serde_json::from_str(&secret)
            .map_err(|error| format!("invalid stored identity: {error}"))?;
        return wf_observer_sdk::restore_identity(secret).map_err(|error| error.to_string());
    }
    let identity = wf_observer_sdk::create_identity();
    let secret =
        serde_json::to_string(&identity.secret_bytes()).map_err(|error| error.to_string())?;
    storage
        .set_item(KEY, &secret)
        .map_err(|error| format!("cannot save identity: {error:?}"))?;
    Ok(identity)
}
