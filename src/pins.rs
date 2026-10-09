use super::*;
impl Eclipse {
    /// Pinned messages open as a dropdown anchored under the header pin icon. Drag its bottom edge to
    /// resize; the height is saved with preferences. Clicking a pin jumps to it in the conversation.
    pub(in crate::ui) fn pins_dropdown(&mut self,ctx:&egui::Context){
        let Some(anchor)=self.pins_anchor else{return;};
        let mut open=true;let just_opened=std::mem::take(&mut self.pins_just_opened);let mut jump=None;
        let max_height=(ctx.screen_rect().bottom()-anchor.bottom()-110.).max(140.);
        egui::Popup::new(egui::Id::new("pins-dropdown"),ctx.clone(),anchor,egui::LayerId::background())
            .open_bool(&mut open).align(egui::RectAlign::BOTTOM_END).gap(8.).width(440.)
            .close_behavior(if just_opened{egui::PopupCloseBehavior::IgnoreClicks}else{egui::PopupCloseBehavior::CloseOnClickOutside})
            .frame(egui::Frame::NONE.fill(SIDE).stroke(Stroke::new(1.0_f32,BORDER)).corner_radius(12).inner_margin(egui::Margin{left:10,right:10,top:10,bottom:2}))
            .show(|ui|{
                ui.set_width(440.);
                ui.label(RichText::new("Pinned Messages").size(15.).strong());
                ui.separator();
                let height=self.prefs.pins_height.min(max_height);
                egui::ScrollArea::vertical().id_salt("pins-list").min_scrolled_height(height).max_height(height).auto_shrink([false,false]).show(ui,|ui|{
                    match self.pins.clone(){
                        None=>{ui.add_space(12.);ui.vertical_centered(|ui|{ui.spinner();ui.weak("Loading pinned messages…");});},
                        Some(pins) if pins.is_empty()=>{ui.add_space(12.);ui.vertical_centered(|ui|{ui.label("This channel doesn't have any pinned messages… yet.");});},
                        Some(pins)=>for message in &pins{
                            let bg=ui.painter().add(egui::Shape::Noop);
                            let card=egui::Frame::NONE.inner_margin(8).show(ui,|ui|{ui.set_min_width(ui.available_width());self.message_ui(ui,message);}).response;
                            let click=ui.interact(card.rect,egui::Id::new(("pin-jump",&message.id)),egui::Sense::click()).on_hover_cursor(egui::CursorIcon::PointingHand);
                            ui.painter().set(bg,egui::Shape::rect_filled(card.rect,8,if click.hovered(){Color32::from_gray(44)}else{CARD}));
                            if click.hovered(){
                                let pill=egui::Rect::from_min_size(card.rect.right_top()+Vec2::new(-52.,6.),Vec2::new(44.,20.));
                                ui.painter().rect_filled(pill,6,Color32::from_gray(64));ui.painter().text(pill.center(),egui::Align2::CENTER_CENTER,"Jump",egui::FontId::proportional(11.),TEXT);
                            }
                            if click.clicked(){jump=Some(message.clone());}
                            ui.add_space(6.);
                        },
                    }
                });
                let (grip,_)=ui.allocate_exact_size(Vec2::new(ui.available_width(),10.),egui::Sense::hover());
                let drag=ui.interact(grip,egui::Id::new("pins-resize"),egui::Sense::drag()).on_hover_cursor(egui::CursorIcon::ResizeVertical).on_hover_text("Drag to resize");
                if drag.dragged(){self.prefs.pins_height=(height+drag.drag_delta().y).clamp(140.,max_height);}
                ui.painter().rect_filled(egui::Rect::from_center_size(grip.center(),Vec2::new(36.,4.)),2,if drag.hovered()||drag.dragged(){Color32::from_gray(150)}else{Color32::from_gray(80)});
            });
        if !open{self.pins_anchor=None;}
        if let Some(message)=jump{self.jump_to_message(&message);}
    }
    /// Scrolls to a message, fetching the history around it when it is not loaded.
    pub(in crate::ui) fn jump_to_message(&mut self,message:&Message){
        self.pins_anchor=None;
        if self.channel.as_ref().is_none_or(|c|c.id!=message.channel_id){return;}
        self.search.clear();
        if self.messages.iter().any(|m|m.id==message.id){self.jump_to=Some((message.id.clone(),4));}
        else if !self.preview{self.send_command(Command::Around(message.channel_id.clone(),message.id.clone()));}
    }
}
