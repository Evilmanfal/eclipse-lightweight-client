use super::*;
use crate::presence::Status;
use crate::widgets::{bar_control, chevron, Control};
pub(in crate::ui) const USER_PANEL_HEIGHT: f32 = 52.0;
impl Eclipse {
    /// Discord-style account bar under the server rail and channel list. Your nameplate is its
    /// background; without one it uses the client's surface colour.
    pub(in crate::ui) fn user_panel(&mut self,ui:&mut egui::Ui){
        let user=self.user.clone().unwrap_or_default();
        let (rect,_)=ui.allocate_exact_size(Vec2::new(ui.available_width(),USER_PANEL_HEIGHT),egui::Sense::hover());
        let radius=if self.compact{0}else{12};
        ui.painter().rect_filled(rect,radius,crate::preferences::color(&self.prefs.theme.surface).unwrap_or(SIDE));
        crate::identity::paint_art_playing(ui,&mut self.images,rect,crate::identity::nameplate(&user),radius);
        ui.scope_builder(egui::UiBuilder::new().max_rect(rect.shrink2(Vec2::new(8.,0.))).layout(egui::Layout::left_to_right(egui::Align::Center)),|ui|{
            ui.spacing_mut().item_spacing.x=2.;
            let avatar=self.paint_avatar(ui,&user,None,34.,true);ui.add_space(6.);
            if avatar.clicked(){self.toggle_account_menu(rect);}
            let controls=if self.prefs.activity_toggle{30.+2.}else{0.}+3.*30.+2.*12.+5.*2.;
            let text=Vec2::new((ui.available_width()-controls).max(20.),38.);
            ui.allocate_ui_with_layout(text,egui::Layout::top_down(egui::Align::Min),|ui|{
                ui.set_width(text.x);ui.shrink_clip_rect(ui.max_rect());ui.spacing_mut().item_spacing.y=0.;
                let label=if self.prefs.streamer{"Hidden".to_owned()}else{user.name().to_owned()};
                if crate::identity::name(ui,&user,&label,15.,TEXT).clicked(){self.toggle_account_menu(rect);}
                ui.add(egui::Label::new(RichText::new(self.own_status_label(&user)).size(11.).color(Color32::from_gray(200))).truncate());
            });
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center),|ui|{
                ui.spacing_mut().item_spacing.x=2.;
                if bar_control(ui,Control::Settings,false,30.,"User Settings").clicked(){self.open_settings();}
                let output=chevron(ui,30.,"Output options");
                egui::Popup::menu(&output).id(egui::Id::new("user-panel-output")).show(|ui|self.device_menu(ui,false));
                if bar_control(ui,Control::Headphones,self.calls.deafened(),30.,if self.calls.deafened(){"Undeafen"}else{"Deafen"}).clicked(){self.calls.toggle_deafen();}
                let input=chevron(ui,30.,"Input options");
                egui::Popup::menu(&input).id(egui::Id::new("user-panel-input")).show(|ui|self.device_menu(ui,true));
                if bar_control(ui,Control::Mic,self.calls.muted(),30.,if self.calls.muted(){"Unmute"}else{"Mute"}).clicked(){self.calls.toggle_mute();}
                if self.prefs.activity_toggle&&bar_control(ui,Control::Activity,!self.game_activity,30.,"Toggle activity sharing").clicked(){self.mutate("save-settings",reqwest::Method::PATCH,"/users/@me/settings".into(),Some(serde_json::json!({"show_current_game":!self.game_activity})));}
            });
        });
    }
    fn own_status_label(&self,user:&User)->String{
        match self.presences.get(&user.id,None){
            Status::Online=>"Online".into(),Status::Idle=>"Idle".into(),Status::Dnd=>"Do Not Disturb".into(),Status::Offline=>"Invisible".into(),
            // Until Discord reports a presence, a healthy connection reads as Online and anything else shows the connection state.
            Status::Unknown=>if self.status=="Live"{"Online".into()}else{self.status.clone()},
        }
    }
    fn device_menu(&mut self,ui:&mut egui::Ui,input:bool){
        ui.set_min_width(240.);
        ui.label(RichText::new(if input{"INPUT DEVICE"}else{"OUTPUT DEVICE"}).size(10.).strong().color(MUTED));
        if self.audio_devices.is_none()&&!self.preview{self.audio_devices=discord_voice::audio::devices().ok();}
        let devices:Vec<(String,String)>=self.audio_devices.as_ref().map(|d|if input{d.inputs.clone()}else{d.outputs.clone()}).unwrap_or_default();
        let selected=if input{&mut self.prefs.input}else{&mut self.prefs.output};
        if ui.selectable_label(selected.is_none(),"System default").clicked(){*selected=None;ui.close();}
        for (id,name) in devices{if ui.selectable_label(selected.as_deref()==Some(id.as_str()),name).clicked(){*selected=Some(id);ui.close();}}
        if self.preview{ui.weak("Devices are not listed in the offline preview.");}
        ui.separator();
        if ui.button("Voice Settings").clicked(){self.open_settings();self.settings_page="Voice & Video".into();self.load_settings_section();ui.close();}
    }
}
