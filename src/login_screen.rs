use super::*;
use crate::login::{self, Mfa, Outcome, Qr, QrSession};
use zeroize::Zeroizing;

#[derive(Clone, Copy, PartialEq)]
pub(in crate::ui) enum LoginTab { Qr, Password }

/// Sign-in screen state. Secrets are wiped as soon as they are sent.
pub(in crate::ui) struct LoginUi {
    tab: LoginTab,
    login: String,
    password: String,
    code: String,
    mfa: Option<Mfa>,
    /// Masked phone number once Discord has sent an SMS code.
    sms_phone: Option<String>,
    pending: Option<crossbeam_channel::Receiver<Outcome>>,
    qr: Option<QrSession>,
    qr_url: Option<String>,
    qr_scanned: Option<String>,
    pub(in crate::ui) remember: bool,
    error: Option<String>,
}
impl Default for LoginUi {
    fn default() -> Self {
        Self { tab: LoginTab::Qr, login: String::new(), password: String::new(), code: String::new(), mfa: None, sms_phone: None, pending: None, qr: None, qr_url: None, qr_scanned: None, remember: true, error: None }
    }
}
impl Drop for LoginUi { fn drop(&mut self) { self.password.zeroize(); self.code.zeroize(); } }

impl Eclipse {
    /// Starts the Discord session for a freshly obtained token; it is saved only once Discord accepts it.
    pub(in crate::ui) fn begin_session(&mut self, token: Zeroizing<String>, ctx: &egui::Context) {
        self.backend = Some(backend::start(token.to_string(), ctx.clone()));
        self.pending_save = self.login_ui.remember.then_some(token);
        self.connecting = true;
        self.error = None;
        self.status = "Connecting…".into();
    }
    /// Signs out and forgets the saved session.
    pub(in crate::ui) fn log_out(&mut self) {
        login::saved::delete();
        self.pending_save = None;
        self.auto_login = false;
        self.login_ui = LoginUi::default();
        self.disconnect();
    }
    pub(in crate::ui) fn login_card(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        self.poll_login(ctx);
        ui.label(RichText::new("Welcome back!").size(22.0).strong());
        ui.label(RichText::new("Sign in with your Discord account.").color(MUTED).size(13.0));
        ui.add_space(10.0);
        if self.connecting {
            ui.horizontal(|ui| { ui.spinner(); ui.label(RichText::new(if self.auto_login { "Signing you back in…" } else { "Connecting to Discord…" }).color(MUTED)); });
            return;
        }
        ui.horizontal(|ui| {
            for (tab, label) in [(LoginTab::Qr, "QR Code"), (LoginTab::Password, "Email or Phone")] {
                if ui.selectable_label(self.login_ui.tab == tab, label).clicked() && self.login_ui.tab != tab {
                    self.login_ui.tab = tab; self.login_ui.error = None;
                }
            }
        });
        ui.add_space(12.0);
        match self.login_ui.tab {
            LoginTab::Qr => self.qr_tab(ui, ctx),
            LoginTab::Password => if self.login_ui.mfa.is_some() { self.mfa_step(ui, ctx) } else { self.password_tab(ui, ctx) },
        }
        if self.login_ui.tab != LoginTab::Qr { self.login_ui.qr = None; self.login_ui.qr_url = None; self.login_ui.qr_scanned = None; }
        ui.add_space(10.0);
        ui.checkbox(&mut self.login_ui.remember, "Stay signed in").on_hover_text("Saved securely in Windows Credential Manager. Log out to remove it.");
        if let Some(error) = self.login_ui.error.clone().or_else(|| self.error.clone()) {
            ui.add_space(6.0);
            ui.colored_label(Color32::from_rgb(255, 160, 151), error);
        }
        ui.add_space(6.0);
        ui.label(RichText::new("Unofficial clients are not supported by Discord.").size(11.0).color(MUTED));
    }
    fn poll_login(&mut self, ctx: &egui::Context) {
        if let Some(outcome) = self.login_ui.pending.as_ref().and_then(|rx| rx.try_recv().ok()) {
            self.login_ui.pending = None;
            match outcome {
                Outcome::Token(token) => { self.login_ui.password.zeroize(); self.login_ui.code.zeroize(); self.login_ui.mfa = None; self.begin_session(token, ctx); }
                Outcome::Mfa(mfa) => { self.login_ui.password.zeroize(); self.login_ui.mfa = Some(mfa); self.login_ui.error = None; }
                Outcome::SmsSent(phone) => { self.login_ui.sms_phone = Some(phone); self.login_ui.error = None; }
                Outcome::Error(error) => { self.login_ui.code.zeroize(); self.login_ui.error = Some(error); }
            }
        }
        let events: Vec<Qr> = self.login_ui.qr.as_ref().map(|s| s.rx.try_iter().collect()).unwrap_or_default();
        for event in events {
            match event {
                Qr::Code(url) => { self.login_ui.qr_url = Some(url); self.login_ui.qr_scanned = None; }
                Qr::Scanned(name) => self.login_ui.qr_scanned = Some(name),
                Qr::Restarting => { self.login_ui.qr_url = None; self.login_ui.qr_scanned = None; }
                Qr::Token(token) => { self.login_ui.qr = None; self.begin_session(token, ctx); return; }
                Qr::Error(error) => { self.login_ui.qr = None; self.login_ui.error = Some(error); }
            }
        }
    }
    fn qr_tab(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        if self.login_ui.qr.is_none() && self.login_ui.error.is_none() { self.login_ui.qr = Some(login::qr(ctx)); }
        ui.horizontal(|ui| {
            let (rect, _) = ui.allocate_exact_size(Vec2::splat(176.0), egui::Sense::hover());
            ui.painter().rect_filled(rect, 10, Color32::WHITE);
            match &self.login_ui.qr_url {
                Some(url) if self.login_ui.qr_scanned.is_none() => paint_qr(ui, rect.shrink(10.0), url),
                Some(_) => { ui.painter().text(rect.center(), egui::Align2::CENTER_CENTER, "✔", egui::FontId::proportional(48.0), Color32::from_rgb(35, 165, 90)); }
                None => { ui.put(egui::Rect::from_center_size(rect.center(), Vec2::splat(24.0)), egui::Spinner::new().color(Color32::DARK_GRAY)); }
            }
            ui.add_space(14.0);
            ui.vertical(|ui| {
                ui.set_width(230.0);
                if let Some(name) = &self.login_ui.qr_scanned {
                    ui.label(RichText::new("Check your phone!").size(17.0).strong());
                    ui.label(RichText::new(format!("Approve the sign-in for {name} in the Discord app.")).color(MUTED));
                } else {
                    ui.label(RichText::new("Log in with QR Code").size(17.0).strong());
                    ui.label(RichText::new("Scan this with the Discord mobile app to log in instantly. No password or 2FA code needed.").color(MUTED));
                }
                if self.login_ui.error.is_some() && ui.button("Try again").clicked() { self.login_ui.error = None; }
            });
        });
    }
    fn password_tab(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        let busy = self.login_ui.pending.is_some();
        ui.label(RichText::new("EMAIL OR PHONE NUMBER").size(11.0).strong().color(MUTED));
        ui.add_enabled(!busy, egui::TextEdit::singleline(&mut self.login_ui.login).desired_width(f32::INFINITY));
        ui.add_space(6.0);
        ui.label(RichText::new("PASSWORD").size(11.0).strong().color(MUTED));
        let field = ui.add_enabled(!busy, egui::TextEdit::singleline(&mut self.login_ui.password).password(true).desired_width(f32::INFINITY));
        ui.hyperlink_to(RichText::new("Forgot your password?").size(12.0), "https://discord.com/login");
        ui.add_space(8.0);
        let ready = !busy && !self.login_ui.login.trim().is_empty() && !self.login_ui.password.is_empty();
        let enter = field.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
        if ui.add_enabled(ready, primary(if busy { "Logging in…" } else { "Log In" })).clicked() || (ready && enter) {
            self.login_ui.error = None;
            let password = Zeroizing::new(std::mem::take(&mut self.login_ui.password));
            self.login_ui.pending = Some(login::password(ctx, self.login_ui.login.trim().to_owned(), password));
        }
    }
    fn mfa_step(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        let Some(mfa) = self.login_ui.mfa.clone() else { return };
        let busy = self.login_ui.pending.is_some();
        ui.label(RichText::new("Multi-Factor Authentication").size(17.0).strong());
        if !mfa.totp && !mfa.sms && !mfa.backup {
            ui.label(RichText::new("This account only allows security keys or passkeys, which need a browser. Use the QR Code tab instead.").color(MUTED));
        } else {
            let sms = self.login_ui.sms_phone.clone();
            ui.label(RichText::new(match &sms {
                Some(phone) => format!("Enter the code Discord texted to {phone}."),
                None if mfa.totp && mfa.backup => "Enter the 6-digit code from your authenticator app, or an 8-character backup code.".into(),
                None if mfa.totp => "Enter the 6-digit code from your authenticator app.".into(),
                None => "Enter one of your 8-character backup codes, or get a code by text message.".into(),
            }).color(MUTED));
            ui.add_space(6.0);
            let field = ui.add_enabled(!busy, egui::TextEdit::singleline(&mut self.login_ui.code).hint_text(if sms.is_some() { "SMS code" } else { "6-digit or backup code" }).desired_width(f32::INFINITY));
            if self.login_ui.code.is_empty() && !busy { field.request_focus(); }
            ui.add_space(8.0);
            let ready = !busy && !self.login_ui.code.trim().is_empty();
            let enter = field.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
            if ui.add_enabled(ready, primary(if busy { "Verifying…" } else { "Log In" })).clicked() || (ready && enter) {
                self.login_ui.error = None;
                let code = std::mem::take(&mut self.login_ui.code);
                self.login_ui.pending = Some(if sms.is_some() { login::sms(ctx, &mfa, code) } else { login::code(ctx, &mfa, code) });
            }
            if mfa.sms && sms.is_none() && ui.add_enabled(!busy, egui::Button::new("Send a code by text message")).clicked() {
                self.login_ui.pending = Some(login::send_sms(ctx, &mfa));
            }
            if mfa.webauthn { ui.label(RichText::new("Security keys and passkeys need a browser; use a code above or the QR Code tab.").size(11.0).color(MUTED)); }
        }
        if ui.small_button("Go back").clicked() { self.login_ui.mfa = None; self.login_ui.sms_phone = None; self.login_ui.code.zeroize(); self.login_ui.error = None; }
    }
}

/// Draws a QR code as dark modules on the white card.
fn paint_qr(ui: &egui::Ui, rect: egui::Rect, text: &str) {
    let Ok(code) = qrcodegen::QrCode::encode_text(text, qrcodegen::QrCodeEcc::Medium) else { return };
    let size = code.size();
    let module = rect.width() / size as f32;
    for y in 0..size {
        for x in 0..size {
            if code.get_module(x, y) {
                let min = rect.min + Vec2::new(x as f32 * module, y as f32 * module);
                ui.painter().rect_filled(egui::Rect::from_min_size(min, Vec2::splat(module + 0.3)), 0, Color32::BLACK);
            }
        }
    }
}
