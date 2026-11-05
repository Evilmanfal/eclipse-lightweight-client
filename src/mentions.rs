//! Mentions: <@id>, <@&role> and <#channel> shown as readable names, and the @ suggestion list in
//! the message box (members, @everyone and @here), like Discord.
use super::*;
use crate::message_media::{mention_token, replace_mentions};

/// One row in the @ suggestion list.
#[derive(Clone)]
pub(in crate::ui) struct Suggestion { pub label: String, pub detail: String, pub user: Option<User>, pub id: Option<String> }

impl Eclipse {
    /// Message text with mentions turned into @names, #channels and @roles.
    /// Each mention is wrapped in MENTION_START/END so the message body can highlight it; use
    /// `strip_marks` where plain text is needed.
    pub(in crate::ui) fn readable(&self, message: &Message, text: &str) -> String {
        use crate::message_media::{MENTION_END as E, MENTION_START as S};
        let resolved = replace_mentions(text, |kind, id| Some(match kind {
            '&' => self.server.roles.iter().find(|r| r.id == id).map(|r| format!("@{}", r.name)).unwrap_or_else(|| "@unknown-role".into()),
            '#' => self.channels.iter().chain(self.dms.iter()).find(|c| c.id == id).map(|c| format!("#{}", c.name.clone().unwrap_or_else(|| c.label()))).unwrap_or_else(|| "#unknown".into()),
            _ => self.mention_name(message, id).map(|name| format!("@{name}")).unwrap_or_else(|| "@unknown-user".into()),
        }).map(|name| format!("{S}{name}{E}")));
        // @everyone and @here are typed as plain words; highlight them when they notify.
        if message.mention_everyone {
            let mut out = String::with_capacity(resolved.len());
            let mut last = 0;
            for range in crate::message_media::mention_ranges(&resolved, &[]) {
                if resolved[..range.start].ends_with(S) { continue; }
                out.push_str(&resolved[last..range.start]); out.push(S); out.push_str(&resolved[range.clone()]); out.push(E); last = range.end;
            }
            out.push_str(&resolved[last..]);
            return out;
        }
        resolved
    }
    fn mention_name(&self, message: &Message, id: &str) -> Option<String> {
        if let Some(member) = self.server.members.get(id).filter(|_| self.guild.as_deref() == Some(self.server.id.as_str())) { return Some(member.name().to_owned()); }
        message.mentions.iter().find(|u| u.id == id)
            .or_else(|| self.user.as_ref().filter(|u| u.id == id))
            .or_else(|| self.friends.iter().find(|r| r.id == id).map(|r| &r.user).filter(|u| !u.username.is_empty()))
            .or_else(|| self.dms.iter().flat_map(|c| c.recipients.iter()).find(|u| u.id == id))
            .or_else(|| self.messages.iter().map(|m| &m.author).find(|u| u.id == id))
            .map(|u| u.name().to_owned())
    }
    /// Who can be mentioned here for a typed query: @everyone / @here (when allowed) and members.
    pub(in crate::ui) fn suggestions(&self, channel: &Channel, query: &str) -> Vec<Suggestion> {
        let query = query.to_lowercase();
        let mut result = vec![];
        let me = self.user.as_ref().map(|u| u.id.as_str()).unwrap_or("");
        if channel.guild_id.is_some() && (self.server.can(me, 17) || self.preview) {
            for (name, detail) in [("everyone", "Notify everyone who has permission to view this channel."), ("here", "Notify everyone online who has permission to view this channel.")] {
                if name.starts_with(&query) { result.push(Suggestion { label: format!("@{name}"), detail: detail.into(), user: None, id: None }); }
            }
        }
        let people: Vec<(User, String)> = if channel.guild_id.is_some() && self.guild.as_deref() == Some(self.server.id.as_str()) {
            self.server.members.values().map(|m| (m.user.clone(), m.name().to_owned())).collect()
        } else {
            channel.recipients.iter().chain(self.user.iter()).map(|u| (u.clone(), u.name().to_owned())).collect()
        };
        let mut matches: Vec<(u8, User, String)> = people.into_iter().filter(|(u, _)| !u.id.is_empty() && !u.username.is_empty()).filter_map(|(user, name)| {
            let (display, username) = (name.to_lowercase(), user.username.to_lowercase());
            let rank = if query.is_empty() || display.starts_with(&query) || username.starts_with(&query) { 0 } else if display.contains(&query) || username.contains(&query) { 1 } else { return None };
            Some((rank, user, name))
        }).collect();
        matches.sort_by(|a, b| a.0.cmp(&b.0).then_with(|| a.2.to_lowercase().cmp(&b.2.to_lowercase())));
        for (_, user, name) in matches.into_iter().take(10 - result.len().min(10)) {
            result.push(Suggestion { label: name, detail: user.username.clone(), id: Some(user.id.clone()), user: Some(user) });
        }
        result
    }
    /// Takes the arrow, Tab and Enter keys before the message box sees them while the list is open.
    pub(in crate::ui) fn mention_keys(&mut self, ctx: &egui::Context) -> (i32, bool) {
        if !self.mention_open { return (0, false); }
        ctx.input_mut(|i| {
            let mut step = 0;
            if i.consume_key(egui::Modifiers::NONE, egui::Key::ArrowDown) { step += 1; }
            if i.consume_key(egui::Modifiers::NONE, egui::Key::ArrowUp) { step -= 1; }
            let choose = i.consume_key(egui::Modifiers::NONE, egui::Key::Tab) | i.consume_key(egui::Modifiers::NONE, egui::Key::Enter);
            (step, choose)
        })
    }
    /// The @ suggestion list above the message box while an @word is being typed.
    pub(in crate::ui) fn mention_popup(&mut self, ctx: &egui::Context, channel: &Channel, response: &egui::Response, (step, choose): (i32, bool)) {
        self.mention_open = false;
        // Pressing a row takes focus from the message box before the click lands, so keep the list
        // alive while the pointer is over where it was drawn last frame.
        let popup_rect = egui::Id::new("mention-popup-rect");
        let over_popup = ctx.data(|d| d.get_temp::<egui::Rect>(popup_rect)).is_some_and(|r| ctx.pointer_latest_pos().is_some_and(|p| r.contains(p)));
        if !response.has_focus() && !over_popup { ctx.data_mut(|d| d.remove::<egui::Rect>(popup_rect)); return; }
        let draft = self.drafts.get(&channel.id).cloned().unwrap_or_default();
        let cursor = egui::text_edit::TextEditState::load(ctx, response.id).and_then(|s| s.cursor.char_range()).map(|r| r.primary.index).unwrap_or_else(|| draft.chars().count());
        let Some((start, query)) = mention_token(&draft, cursor) else { self.mention_pick = 0; return };
        // Ask Discord for matching members; large servers don't send everyone up front.
        if let Some(guild) = channel.guild_id.clone().filter(|_| !self.preview && !query.is_empty() && query != self.mention_query) {
            self.mention_query = query.clone();
            self.send_gateway(serde_json::json!({"op":8,"d":{"guild_id":guild,"query":query,"limit":10,"presences":false}}));
        }
        let items = self.suggestions(channel, &query);
        if items.is_empty() { return; }
        self.mention_open = true;
        self.mention_pick = (self.mention_pick as i32 + step).rem_euclid(items.len() as i32) as usize;
        let mut picked = choose.then_some(self.mention_pick);
        let row = 40.0;
        // Anchored by its bottom edge so it always sits just above the message box.
        let at = egui::pos2(response.rect.left() - 8.0, response.rect.top() - 20.0);
        let area = egui::Area::new(egui::Id::new("mention-popup")).order(egui::Order::Foreground).pivot(egui::Align2::LEFT_BOTTOM).fixed_pos(at).show(ctx, |ui| {
            egui::Frame::NONE.fill(Color32::from_gray(30)).stroke(Stroke::new(1.0_f32, BORDER)).corner_radius(8).inner_margin(6).show(ui, |ui| {
                ui.set_width(response.rect.width().max(320.0));
                ui.label(RichText::new("MEMBERS").size(11.0).strong().color(MUTED));
                for (index, item) in items.iter().enumerate() {
                    let (rect, row_response) = ui.allocate_exact_size(Vec2::new(ui.available_width(), row - 4.0), egui::Sense::click());
                    if row_response.hovered() && ui.input(|i| i.pointer.delta() != Vec2::ZERO) { self.mention_pick = index; }
                    if index == self.mention_pick { ui.painter().rect_filled(rect, 6, Color32::from_gray(48)); }
                    let avatar = egui::Rect::from_center_size(egui::pos2(rect.left() + 20.0, rect.center().y), Vec2::splat(26.0));
                    if let Some(user) = &item.user {
                        let url = if self.preview { Some(format!("demo://user/{}", user.id)) } else { assets::avatar_url(user, self.guild.as_deref(), None) };
                        ui.painter().circle_filled(avatar.center(), 13.0, Color32::from_gray(60));
                        if let Some(texture) = url.and_then(|u| self.images.texture(&u, avatar, ctx)) { egui::Image::new((texture, avatar.size())).corner_radius(13).paint_at(ui, avatar); }
                    } else {
                        ui.painter().text(avatar.center(), egui::Align2::CENTER_CENTER, "@", egui::FontId::proportional(18.0), TEXT);
                    }
                    let name = ui.painter().text(egui::pos2(avatar.right() + 10.0, rect.center().y), egui::Align2::LEFT_CENTER, &item.label, egui::FontId::proportional(14.0), TEXT);
                    let detail = ui.painter().layout(item.detail.clone(), egui::FontId::proportional(12.0), MUTED, (rect.right() - name.right() - 20.0).max(40.0));
                    ui.painter().galley(egui::pos2(rect.right() - detail.size().x - 8.0, rect.center().y - detail.size().y / 2.0), detail, MUTED);
                    if row_response.on_hover_cursor(egui::CursorIcon::PointingHand).clicked() { picked = Some(index); }
                }
            });
        });
        ctx.data_mut(|d| d.insert_temp(popup_rect, area.response.rect));
        let Some(item) = picked.and_then(|i| items.get(i).cloned()) else { return };
        ctx.data_mut(|d| d.remove::<egui::Rect>(popup_rect));
        // Members go in as @username (sent as <@id>); @everyone and @here are sent as typed.
        let inserted = match (&item.user, &item.id) { (Some(user), Some(id)) => { let label = format!("@{}", user.username); self.mention_ids.insert(label.clone(), id.clone()); label }, _ => item.label.clone() };
        let chars: Vec<char> = draft.chars().collect();
        let mut text: String = chars[..start].iter().collect();
        text.push_str(&inserted); text.push(' ');
        let end = text.chars().count();
        text.extend(chars[cursor.min(chars.len())..].iter());
        self.drafts.insert(channel.id.clone(), text);
        let mut state = egui::text_edit::TextEditState::load(ctx, response.id).unwrap_or_default();
        state.cursor.set_char_range(Some(egui::text::CCursorRange::one(egui::text::CCursor::new(end))));
        state.store(ctx, response.id);
        response.request_focus();
        self.mention_open = false; self.mention_pick = 0;
    }
}
