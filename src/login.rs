//! Discord sign-in without a browser: QR code (approve in the Discord mobile app), email/phone and
//! password with every 2FA prompt Discord can return, and a saved session in Windows Credential
//! Manager so Eclipse stays signed in. Tokens are held in zeroizing memory and never logged.
use crossbeam_channel::{unbounded, Receiver, Sender};
use eframe::egui;
use reqwest::blocking::Client;
use serde_json::{json, Value};
use std::{
    sync::{atomic::{AtomicBool, Ordering}, Arc},
    thread,
    time::{Duration, Instant},
};
use zeroize::Zeroizing;

const API: &str = "https://discord.com/api/v9";

/// What one sign-in step produced.
pub enum Outcome {
    Token(Zeroizing<String>),
    /// Discord wants a second factor for this `ticket`.
    Mfa(Mfa),
    /// Discord sent an SMS code; the phone number is masked by Discord.
    SmsSent(String),
    Error(String),
}

#[derive(Clone, Debug, Default)]
pub struct Mfa {
    pub ticket: Zeroizing<String>,
    pub totp: bool,
    pub backup: bool,
    pub sms: bool,
    /// Security keys and passkeys need a browser and are not supported natively.
    pub webauthn: bool,
    /// Ties the 2FA step to the password step on newer Discord versions.
    pub login_instance_id: Option<String>,
}
impl Mfa {
    /// The body Discord's own client sends with a 2FA code.
    fn body(&self, code: &str) -> Value {
        let mut body = json!({ "code": code, "ticket": self.ticket.as_str(), "login_source": null, "gift_code_sku_id": null });
        if let Some(id) = &self.login_instance_id { body["login_instance_id"] = json!(id); }
        body
    }
}

/// One client for every sign-in step, so Discord's session cookies from the password step come
/// back with the 2FA step. Discord rejects otherwise-correct codes from an unrelated session.
fn client() -> Result<&'static Client, String> {
    static CLIENT: std::sync::OnceLock<Client> = std::sync::OnceLock::new();
    if let Some(client) = CLIENT.get() { return Ok(client); }
    let client = Client::builder()
        .timeout(Duration::from_secs(20))
        .user_agent("Eclipse Native")
        .cookie_store(true)
        .build()
        .map_err(|_| "Could not start a secure connection to Discord.".to_owned())?;
    Ok(CLIENT.get_or_init(|| client))
}

/// Discord's browser fingerprint for this sign-in session, sent as X-Fingerprint on every step.
fn fingerprint(client: &Client) -> Option<String> {
    static FINGERPRINT: std::sync::Mutex<Option<String>> = std::sync::Mutex::new(None);
    let mut cached = FINGERPRINT.lock().ok()?;
    if cached.is_none() {
        let value: Value = client.get(format!("{API}/experiments")).send().ok()?.json().ok()?;
        *cached = value["fingerprint"].as_str().filter(|f| f.len() < 128).map(str::to_owned);
    }
    cached.clone()
}

fn post(route: &str, body: Value) -> Result<Value, (u16, Value)> {
    let client = client().map_err(|e| (0, json!({ "message": e })))?;
    let mut request = client.post(format!("{API}{route}")).json(&body);
    if let Some(fingerprint) = fingerprint(client) { request = request.header("X-Fingerprint", fingerprint); }
    let response = request.send().map_err(|_| (0, json!({ "message": "Discord could not be reached. Check your connection." })))?;
    let status = response.status().as_u16();
    let value: Value = response.json().unwrap_or(Value::Null);
    if (200..300).contains(&status) { Ok(value) } else { Err((status, value)) }
}

/// Turns a Discord error body into one readable sentence (never echoing credentials).
fn explain(status: u16, body: &Value) -> String {
    if body.get("captcha_key").is_some() || body.get("captcha_sitekey").is_some() {
        return "Discord asked for a captcha, which only works in a browser. Use the QR code instead.".into();
    }
    let field = ["login", "password", "code", "ticket"].iter().find_map(|f| body["errors"][f]["_errors"][0]["message"].as_str());
    if let Some(message) = field { return message.to_owned(); }
    if status == 429 { return "Too many attempts. Wait a moment and try again.".into(); }
    body["message"].as_str().filter(|m| m.len() < 300).unwrap_or("Discord rejected the sign-in.").to_owned()
}

fn outcome(result: Result<Value, (u16, Value)>) -> Outcome {
    match result {
        Ok(body) => {
            if let Some(token) = body["token"].as_str().filter(|t| !t.is_empty()) { return Outcome::Token(Zeroizing::new(token.to_owned())); }
            if body["mfa"] == true {
                if let Some(ticket) = body["ticket"].as_str() {
                    return Outcome::Mfa(Mfa {
                        ticket: Zeroizing::new(ticket.to_owned()),
                        totp: body["totp"] == true,
                        backup: body["backup"] == true,
                        sms: body["sms"] == true,
                        webauthn: body["webauthn"].is_string(),
                        login_instance_id: body["login_instance_id"].as_str().filter(|id| id.len() < 128).map(str::to_owned),
                    });
                }
            }
            if let Some(phone) = body["phone"].as_str() { return Outcome::SmsSent(phone.to_owned()); }
            Outcome::Error("Discord returned an unexpected sign-in response.".into())
        }
        Err((status, body)) => Outcome::Error(explain(status, &body)),
    }
}

/// Runs a blocking sign-in step on a worker thread and wakes the UI with its result.
fn spawn(ctx: &egui::Context, work: impl FnOnce() -> Outcome + Send + 'static) -> Receiver<Outcome> {
    let (tx, rx) = unbounded();
    let ctx = ctx.clone();
    thread::spawn(move || { let _ = tx.send(work()); ctx.request_repaint(); });
    rx
}

pub fn password(ctx: &egui::Context, login: String, password: Zeroizing<String>) -> Receiver<Outcome> {
    spawn(ctx, move || outcome(post("/auth/login", json!({ "login": login, "password": password.as_str(), "undelete": false }))))
}
/// Authenticator-app codes and 8-character backup codes.
pub fn code(ctx: &egui::Context, mfa: &Mfa, code: String) -> Receiver<Outcome> {
    let mfa = mfa.clone();
    let compact: String = code.chars().filter(|c| !c.is_whitespace() && *c != '-').collect();
    // Discord's client sends authenticator and 8-character backup codes to the same endpoint.
    spawn(ctx, move || outcome(post("/auth/mfa/totp", mfa.body(&compact))))
}
pub fn send_sms(ctx: &egui::Context, mfa: &Mfa) -> Receiver<Outcome> {
    let ticket = mfa.ticket.clone();
    spawn(ctx, move || outcome(post("/auth/mfa/sms/send", json!({ "ticket": ticket.as_str() }))))
}
pub fn sms(ctx: &egui::Context, mfa: &Mfa, code: String) -> Receiver<Outcome> {
    let mfa = mfa.clone();
    spawn(ctx, move || outcome(post("/auth/mfa/sms", mfa.body(code.trim()))))
}

/// Progress of a QR-code sign-in.
pub enum Qr {
    /// Show this URL as a QR code.
    Code(String),
    /// Scanned; waiting for approval in the app. Holds the Discord username.
    Scanned(String),
    Token(Zeroizing<String>),
    /// The code expired or was cancelled; a new one is being requested.
    Restarting,
    Error(String),
}

pub struct QrSession { pub rx: Receiver<Qr>, stop: Arc<AtomicBool> }
impl Drop for QrSession { fn drop(&mut self) { self.stop.store(true, Ordering::Release); } }

/// Discord's remote-auth handshake: the phone app approves a key this client generated, and the
/// token comes back encrypted to that key.
pub fn qr(ctx: &egui::Context) -> QrSession {
    let (tx, rx) = unbounded();
    let stop = Arc::new(AtomicBool::new(false));
    let (ctx, worker_stop) = (ctx.clone(), stop.clone());
    thread::spawn(move || {
        let mut failures = 0;
        while !worker_stop.load(Ordering::Acquire) {
            match remote_auth(&tx, &ctx, &worker_stop) {
                Ok(true) => return,
                Ok(false) => { failures = 0; let _ = tx.send(Qr::Restarting); }
                Err(error) => {
                    failures += 1;
                    if failures >= 3 { let _ = tx.send(Qr::Error(error)); ctx.request_repaint(); return; }
                    thread::sleep(Duration::from_secs(2));
                }
            }
            ctx.request_repaint();
        }
    });
    QrSession { rx, stop }
}

/// Returns Ok(true) once a token was delivered, Ok(false) when the code expired or was cancelled.
fn remote_auth(tx: &Sender<Qr>, ctx: &egui::Context, stop: &AtomicBool) -> Result<bool, String> {
    use base64::{engine::general_purpose::{STANDARD, URL_SAFE_NO_PAD}, Engine};
    use rsa::{pkcs8::EncodePublicKey, Oaep, RsaPrivateKey};
    use tungstenite::{client::IntoClientRequest, stream::MaybeTlsStream, Message};
    let key = RsaPrivateKey::new(&mut rsa::rand_core::OsRng, 2048).map_err(|_| "Could not create a sign-in key.".to_owned())?;
    let public = key.to_public_key().to_public_key_der().map_err(|_| "Could not create a sign-in key.".to_owned())?;
    let decrypt = |data: &str| -> Result<Vec<u8>, String> {
        let bytes = STANDARD.decode(data).map_err(|_| "Discord sent an unreadable sign-in message.".to_owned())?;
        key.decrypt(Oaep::new::<sha2::Sha256>(), &bytes).map_err(|_| "Could not decrypt Discord's sign-in message.".to_owned())
    };
    let mut request = "wss://remote-auth-gateway.discord.gg/?v=2".into_client_request().map_err(|_| "Invalid sign-in address.".to_owned())?;
    request.headers_mut().insert("Origin", "https://discord.com".parse().expect("static header"));
    let (mut socket, _) = tungstenite::connect(request).map_err(|_| "Could not reach Discord's QR sign-in service.".to_owned())?;
    match socket.get_mut() {
        MaybeTlsStream::Plain(stream) => { let _ = stream.set_read_timeout(Some(Duration::from_millis(500))); }
        MaybeTlsStream::Rustls(stream) => { let _ = stream.sock.set_read_timeout(Some(Duration::from_millis(500))); }
        _ => {}
    }
    let send = |socket: &mut tungstenite::WebSocket<MaybeTlsStream<std::net::TcpStream>>, value: Value| socket.send(Message::text(value.to_string())).map_err(|_| "Lost the QR sign-in connection.".to_owned());
    let mut heartbeat = Duration::from_secs(40);
    let mut last_beat = Instant::now();
    loop {
        if stop.load(Ordering::Acquire) { let _ = socket.close(None); return Ok(true); }
        if last_beat.elapsed() >= heartbeat { send(&mut socket, json!({ "op": "heartbeat" }))?; last_beat = Instant::now(); }
        let message = match socket.read() {
            Ok(message) => message,
            Err(tungstenite::Error::Io(error)) if matches!(error.kind(), std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut) => continue,
            Err(tungstenite::Error::ConnectionClosed | tungstenite::Error::AlreadyClosed) => return Ok(false),
            Err(_) => return Err("Lost the QR sign-in connection.".into()),
        };
        let Message::Text(text) = message else { if matches!(message, Message::Close(_)) { return Ok(false); } continue };
        let data: Value = serde_json::from_str(text.as_str()).unwrap_or(Value::Null);
        match data["op"].as_str().unwrap_or("") {
            "hello" => {
                heartbeat = Duration::from_millis(data["heartbeat_interval"].as_u64().unwrap_or(40_000).clamp(5_000, 120_000));
                send(&mut socket, json!({ "op": "init", "encoded_public_key": STANDARD.encode(public.as_bytes()) }))?;
            }
            "nonce_proof" => {
                let nonce = decrypt(data["encrypted_nonce"].as_str().unwrap_or(""))?;
                send(&mut socket, json!({ "op": "nonce_proof", "nonce": URL_SAFE_NO_PAD.encode(nonce) }))?;
            }
            "pending_remote_init" => {
                let fingerprint = data["fingerprint"].as_str().filter(|f| f.len() < 200 && f.bytes().all(|c| c.is_ascii_alphanumeric() || c == b'-' || c == b'_')).ok_or("Discord sent an invalid QR code.")?;
                let _ = tx.send(Qr::Code(format!("https://discord.com/ra/{fingerprint}")));
            }
            "pending_ticket" => {
                let payload = String::from_utf8(decrypt(data["encrypted_user_payload"].as_str().unwrap_or(""))?).unwrap_or_default();
                // "id:discriminator:avatar:username"
                let username = payload.splitn(4, ':').nth(3).unwrap_or("your account").chars().take(64).collect();
                let _ = tx.send(Qr::Scanned(username));
            }
            "pending_login" => {
                let ticket = data["ticket"].as_str().unwrap_or("");
                let body = post("/users/@me/remote-auth/login", json!({ "ticket": ticket })).map_err(|(status, body)| explain(status, &body))?;
                let token = Zeroizing::new(String::from_utf8(decrypt(body["encrypted_token"].as_str().unwrap_or(""))?).map_err(|_| "Discord sent an unreadable token.".to_owned())?);
                let _ = tx.send(Qr::Token(token));
                let _ = socket.close(None);
                return Ok(true);
            }
            "cancel" => return Ok(false),
            _ => {}
        }
        ctx.request_repaint();
    }
}

/// Windows Credential Manager storage for the signed-in session ("Eclipse Discord session").
pub mod saved {
    use zeroize::Zeroizing;
    #[cfg(windows)]
    use windows_sys::Win32::{Foundation::FILETIME, Security::Credentials::{CredDeleteW, CredFree, CredReadW, CredWriteW, CREDENTIALW, CRED_PERSIST_LOCAL_MACHINE, CRED_TYPE_GENERIC}};
    #[cfg(windows)]
    fn target() -> Vec<u16> { "Eclipse/Discord session".encode_utf16().chain([0]).collect() }

    #[cfg(windows)]
    pub fn save(token: &str) -> bool {
        let mut target = target();
        let mut user: Vec<u16> = "Eclipse".encode_utf16().chain([0]).collect();
        let mut blob = Zeroizing::new(token.as_bytes().to_vec());
        let credential = CREDENTIALW {
            Flags: 0, Type: CRED_TYPE_GENERIC, TargetName: target.as_mut_ptr(), Comment: std::ptr::null_mut(),
            LastWritten: FILETIME { dwLowDateTime: 0, dwHighDateTime: 0 },
            CredentialBlobSize: blob.len() as u32, CredentialBlob: blob.as_mut_ptr(), Persist: CRED_PERSIST_LOCAL_MACHINE,
            AttributeCount: 0, Attributes: std::ptr::null_mut(), TargetAlias: std::ptr::null_mut(), UserName: user.as_mut_ptr(),
        };
        unsafe { CredWriteW(&credential, 0) != 0 }
    }
    #[cfg(windows)]
    pub fn load() -> Option<Zeroizing<String>> {
        let target = target();
        let mut credential: *mut CREDENTIALW = std::ptr::null_mut();
        unsafe {
            if CredReadW(target.as_ptr(), CRED_TYPE_GENERIC, 0, &mut credential) == 0 || credential.is_null() { return None; }
            let c = &*credential;
            let bytes = Zeroizing::new(std::slice::from_raw_parts(c.CredentialBlob, c.CredentialBlobSize as usize).to_vec());
            CredFree(credential.cast());
            String::from_utf8(bytes.to_vec()).ok().filter(|t| !t.is_empty()).map(Zeroizing::new)
        }
    }
    #[cfg(windows)]
    pub fn delete() { let target = target(); unsafe { CredDeleteW(target.as_ptr(), CRED_TYPE_GENERIC, 0); } }
    #[cfg(not(windows))] pub fn save(_: &str) -> bool { false }
    #[cfg(not(windows))] pub fn load() -> Option<Zeroizing<String>> { None }
    #[cfg(not(windows))] pub fn delete() {}
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn sign_in_responses_map_to_token_mfa_and_readable_errors() {
        assert!(matches!(outcome(Ok(json!({"token":"abc"}))), Outcome::Token(t) if t.as_str() == "abc"));
        let Outcome::Mfa(mfa) = outcome(Ok(json!({"mfa":true,"ticket":"t1","totp":true,"sms":true,"backup":true,"webauthn":null}))) else { panic!("mfa") };
        assert!(mfa.totp && mfa.sms && mfa.backup && !mfa.webauthn && mfa.ticket.as_str() == "t1");
        assert!(matches!(outcome(Ok(json!({"phone":"+*******1234"}))), Outcome::SmsSent(p) if p.ends_with("1234")));
        let Outcome::Error(captcha) = outcome(Err((400, json!({"captcha_key":["captcha-required"]})))) else { panic!() };
        assert!(captcha.contains("QR"));
        let Outcome::Error(field) = outcome(Err((400, json!({"errors":{"password":{"_errors":[{"message":"Login or password is invalid."}]}}})))) else { panic!() };
        assert_eq!(field, "Login or password is invalid.");
        let Outcome::Error(code) = outcome(Err((400, json!({"message":"Invalid two-factor code","code":60008})))) else { panic!() };
        assert_eq!(code, "Invalid two-factor code");
        let Outcome::Error(ticket) = outcome(Err((400, json!({"errors":{"ticket":{"_errors":[{"code":"MFA_INVALID_TICKET","message":"Invalid two-factor auth ticket"}]}}})))) else { panic!() };
        assert_eq!(ticket, "Invalid two-factor auth ticket");
        let mfa = Mfa { ticket: Zeroizing::new("t".into()), login_instance_id: Some("i".into()), ..Default::default() };
        let body = mfa.body("123456");
        assert_eq!((body["code"].as_str(), body["ticket"].as_str(), body["login_instance_id"].as_str()), (Some("123456"), Some("t"), Some("i")));
        assert!(body["login_source"].is_null() && body.get("gift_code_sku_id").is_some());
    }
}
