use eframe::egui::{self,Color32,RichText,Vec2};
/// Shared, bounded layout for account and server settings. The backdrop owns outside clicks.
pub fn shell(ctx:&egui::Context,id:&str,title:&str,subtitle:&str,content:impl FnOnce(&mut egui::Ui,Vec2))->bool{
    let size=Vec2::new((ctx.screen_rect().width()-100.).clamp(650.,1040.),(ctx.screen_rect().height()-88.).clamp(400.,780.));
    let mut close=false;
    let response=egui::Modal::new(egui::Id::new(id)).backdrop_color(Color32::from_black_alpha(175))
        .frame(egui::Frame::NONE.fill(Color32::from_gray(23)).corner_radius(20).stroke(egui::Stroke::new(1_f32,Color32::from_gray(48))))
        .show(ctx,|ui|{
            ui.set_width(size.x);ui.set_height(size.y);
            egui::Frame::NONE.inner_margin(egui::Margin::symmetric(20,12)).show(ui,|ui|{
                ui.horizontal(|ui|{ui.vertical(|ui|{ui.label(RichText::new(title).size(22.).strong());ui.label(RichText::new(subtitle).size(12.).color(Color32::from_gray(170)));});
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center),|ui|{if ui.add_sized([34.,34.],egui::Button::new(RichText::new("×").size(24.))).on_hover_text("Close settings · Escape").clicked(){close=true;}});
                });
            });
            ui.separator();content(ui,Vec2::new(size.x,size.y-100.));
        });
    close||response.should_close()
}
pub fn card(ui:&mut egui::Ui,content:impl FnOnce(&mut egui::Ui)){
    egui::Frame::NONE.fill(Color32::from_gray(29)).corner_radius(14).inner_margin(16).show(ui,|ui|{ui.vertical(|ui|{ui.set_width(ui.available_width());ui.spacing_mut().item_spacing=Vec2::new(10.,10.);content(ui);});});
}
pub fn description(page:&str)->&'static str{match page{
    "Account & Profile"=>"Make your profile feel like you.","Voice & Video"=>"Choose your devices, input mode and call quality.",
    "Appearance"|"Themes"=>"Make Eclipse comfortable for your eyes.","Performance"=>"Balance smooth animations and memory use.",
    "Devices"=>"Review the devices signed into your account.","Connections"=>"Your connected services and accounts.",
    "Authorized Apps"=>"Apps you have allowed to access your account.","Keybinds"=>"Shortcuts that keep you in control.",
    "Notifications"=>"Choose what you hear and what gets your attention.","Plugins"=>"Optional native features, built into Eclipse.",
    "Roles"=>"Organize your community and choose what each role can do.","Overview"=>"Your server's identity and everyday defaults.",
    "Members"=>"Manage your community and member roles.","Channels"=>"Organize the places where your community talks.",
    _=>"Your preferences, all in one place.",
}}
#[cfg(test)]mod tests{use super::*;
    #[test]fn settings_backdrop_dismisses_but_content_click_does_not(){
        let ctx=egui::Context::default();let mut rect=egui::Rect::NOTHING;
        let input=|events|egui::RawInput{screen_rect:Some(egui::Rect::from_min_size(egui::Pos2::ZERO,Vec2::new(1080.,680.))),events,..Default::default()};
        for _ in 0..3{let _=ctx.run(input(vec![]),|ctx|{assert!(!shell(ctx,"test-settings","Settings","Preferences",|ui,_|{rect=ui.max_rect();ui.label("Content");}));});}
        for (point,expected) in [(rect.center(),false),(egui::pos2(10.,10.),true)]{
            let events=vec![egui::Event::PointerMoved(point),egui::Event::PointerButton{pos:point,button:egui::PointerButton::Primary,pressed:true,modifiers:Default::default()},egui::Event::PointerButton{pos:point,button:egui::PointerButton::Primary,pressed:false,modifiers:Default::default()}];
            let _=ctx.run(input(events),|ctx|{assert_eq!(shell(ctx,"test-settings","Settings","Preferences",|ui,_|{ui.label("Content");}),expected);});
        }
    }
}
