use super::*;
impl Eclipse {
    pub(in crate::ui) fn shop_content(&mut self,ui:&mut egui::Ui){
        ui.horizontal(|ui|{ui.heading("Shop");ui.with_layout(egui::Layout::right_to_left(egui::Align::Center),|ui|{ui.hyperlink_to("Open Discord Shop ↗","https://discord.com/shop");if ui.small_button("Refresh").clicked(){self.request_feature("shop","/collectibles-shop?include_bundles=true".into());}});});
        ui.add_space(14.);
        store_hero(ui,&mut self.images,"MAKE IT YOURS","A new look. All you.","Discover avatar decorations, profile effects, and nameplates.",Color32::from_rgb(75,49,137),Color32::from_rgb(136,74,147),false);
        ui.add_space(18.);ui.horizontal_wrapped(|ui|{for tab in ["All","Avatar decorations","Profile effects","Nameplates","Bundles"]{ui.selectable_value(&mut self.shop_filter,tab.into(),tab);}});ui.add_space(12.);ui.separator();ui.add_space(12.);self.feature_status(ui,"shop");
        let data=self.features.get("shop").cloned().unwrap_or_else(||if self.preview{sample_catalog()}else{Value::Null});
        if let Some(categories)=data["categories"].as_array().or_else(||data.as_array()){
            let mut total=0;
            for category in categories.iter().take(50){
                let products:Vec<_>=category["products"].as_array().into_iter().flatten().filter(|p|matches_product(p,&self.shop_filter)).take(100).cloned().collect();
                if products.is_empty(){continue;}total+=products.len();ui.heading(category["name"].as_str().unwrap_or("Featured collection"));
                if let Some(summary)=category["summary"].as_str(){ui.weak(summary);}ui.add_space(12.);
                if let Some(url)=category_banner(category){let(rect,_)=ui.allocate_exact_size(Vec2::new(ui.available_width(),(ui.available_width()/3.4).clamp(100.,210.)),egui::Sense::hover());crate::identity::paint_art(ui,&mut self.images,rect,Some(url),10);ui.add_space(12.);}
                let cols=((ui.available_width()+14.)/235.).floor().clamp(1.,3.)as usize;
                for row in products.chunks(cols){ui.columns(cols,|columns|{for(ui,product)in columns.iter_mut().zip(row){self.product_card(ui,product);}});ui.add_space(14.);}
                ui.add_space(10.);
            }
            if total==0{ui.weak("No items in this category. Try All or refresh the catalog.");}
        }else if !self.feature_pending.contains("shop"){empty_state(ui,"Your next look is waiting","Refresh the catalog to browse your available collections.");}
    }
    fn product_card(&mut self,ui:&mut egui::Ui,product:&Value){
        egui::Frame::NONE.fill(CARD).corner_radius(12).stroke(Stroke::new(1.0_f32,BORDER)).inner_margin(12).show(ui,|ui|{
            ui.set_min_width(ui.available_width());let(rect,_)=ui.allocate_exact_size(Vec2::new(ui.available_width(),150.),egui::Sense::hover());
            let kind=product["items"][0]["type"].as_u64().unwrap_or(0);
            let tint=if kind==1{Color32::from_rgb(65,54,104)}else if kind==2{Color32::from_rgb(38,72,87)}else{Color32::from_rgb(58,47,83)};
            ui.painter().rect_filled(rect,9,tint);
            let effects=self.features.get("profile-effects").cloned().unwrap_or(Value::Null);
            // Cards stay still until hovered, like Discord; a grid of animated previews would overflow the image cache.
            let image=product_image(product,&effects,self.prefs.animations&&!self.prefs.reduced_motion&&ui.rect_contains_pointer(rect));
            if kind==1&&!self.preview&&!self.features.contains_key("profile-effects")&&!self.feature_errors.contains_key("profile-effects"){self.request_feature("profile-effects","/user-profile-effects".into());}
            if let Some(background)=product["preview_assets"]["bg_static"].as_str().and_then(collectible_asset){crate::identity::paint_art(ui,&mut self.images,rect,Some(background),9);}
            // Off-screen cards must not request art: a large catalog would churn the shared image cache.
            if let Some(url)=image.filter(|_|ui.is_rect_visible(rect)){
                if let Some(texture)=self.images.texture_hover(&url,rect,ui.ctx()){
                    let dimensions=self.images.dimensions(&url,rect.size(),ui.ctx()).unwrap_or(rect.size());let size=dimensions*((rect.width()-16.)/dimensions.x).min((rect.height()-16.)/dimensions.y);
                    let art=egui::Rect::from_center_size(rect.center(),size);egui::Image::new((texture,size)).corner_radius(7).paint_at(ui,art);
                }else{ui.painter().text(rect.center(),egui::Align2::CENTER_CENTER,if self.images.failed(&url){"Artwork unavailable"}else{"Loading artwork…"},egui::FontId::proportional(12.),MUTED);}
            }else if self.preview{sample_art(ui,rect,kind);}else{ui.painter().text(rect.center(),egui::Align2::CENTER_CENTER,"Artwork unavailable",egui::FontId::proportional(12.),MUTED);}
            ui.add_space(8.);ui.add(egui::Label::new(RichText::new(product["name"].as_str().unwrap_or("Collectible")).size(16.).strong()).truncate());
            ui.add(egui::Label::new(RichText::new(product["summary"].as_str().unwrap_or("")).size(12.).color(MUTED)).truncate());
            ui.label(RichText::new(match kind{1=>"PROFILE EFFECT",2=>"NAMEPLATE",1000=>"BUNDLE",_=>"AVATAR DECORATION"}).size(9.).strong().color(MUTED));
            ui.add_space(6.);
            if self.preview{ui.add_enabled(false,egui::Button::new("Sample preview"));}
            else{ui.hyperlink_to("View item ↗",format!("https://discord.com/shop#{}",product["sku_id"].as_str().filter(|id|community::snowflake(id)).unwrap_or("")));}
        });
    }
    pub(in crate::ui) fn quests_content(&mut self,ui:&mut egui::Ui){
        ui.horizontal(|ui|{ui.heading("Quests");ui.with_layout(egui::Layout::right_to_left(egui::Align::Center),|ui|{ui.hyperlink_to("Open Discord Quests ↗","https://discord.com/quests");if ui.small_button("Refresh").clicked(){self.request_feature("quests","/quests/@me".into());}});});ui.add_space(14.);
        store_hero(ui,&mut self.images,"PLAY. EXPLORE. GET REWARDED.","Your next adventure starts here.","Discover experiences and earn rewards with Discord Quests.",Color32::from_rgb(40,49,111),Color32::from_rgb(62,91,134),true);
        ui.add_space(18.);ui.horizontal(|ui|{for tab in ["Discover","Accepted","Completed"]{ui.selectable_value(&mut self.quest_filter,tab.into(),tab);}});ui.add_space(12.);ui.separator();ui.add_space(12.);
        self.feature_status(ui,"quests");self.feature_status(ui,"quest-enroll");
        let data=self.features.get("quests").cloned().unwrap_or_else(||if self.preview{sample_quests()}else{Value::Null});
        if let Some(all)=data["quests"].as_array(){
            let quests:Vec<_>=all.iter().filter(|q|match self.quest_filter.as_str(){"Accepted"=>!q["user_status"]["enrolled_at"].is_null()&&q["user_status"]["completed_at"].is_null(),"Completed"=>!q["user_status"]["completed_at"].is_null(),_=>true}).take(100).cloned().collect();
            if quests.is_empty(){empty_state(ui,"You're all caught up",if self.quest_filter=="Accepted"{"Quests you accept will appear here."}else{"Check back for more quests available to your account."});}
            let cols=if ui.available_width()>610.{2}else{1};
            for row in quests.chunks(cols){ui.columns(cols,|columns|{for(ui,quest)in columns.iter_mut().zip(row){self.quest_card(ui,quest);}});ui.add_space(16.);}
        }else if !self.feature_pending.contains("quests"){empty_state(ui,"Find your next quest","Refresh to load quests and progress from your Discord account.");}
        ui.add_space(12.);ui.label(RichText::new("Complete tasks and claim rewards in Discord. Progress is supplied by your account.").size(11.).color(MUTED));
    }
    fn quest_card(&mut self,ui:&mut egui::Ui,quest:&Value){
        let config=&quest["config"];
        egui::Frame::NONE.fill(CARD).corner_radius(12).stroke(Stroke::new(1.0_f32,BORDER)).inner_margin(14).show(ui,|ui|{
            ui.set_min_width(ui.available_width());let(rect,_)=ui.allocate_exact_size(Vec2::new(ui.available_width(),160.),egui::Sense::hover());
            ui.painter().rect_filled(rect,8,Color32::from_rgb(36,55,79));
            let url=quest_hero(quest);
            if url.is_some(){crate::identity::paint_art(ui,&mut self.images,rect,url,8);}else if self.preview{sample_art(ui,rect,1);}else{ui.painter().text(rect.center(),egui::Align2::CENTER_CENTER,"Artwork unavailable",egui::FontId::proportional(12.),MUTED);}
            ui.add_space(10.);ui.label(RichText::new(config["messages"]["game_title"].as_str().unwrap_or("DISCORD QUEST")).size(10.).strong().color(MUTED));
            ui.add(egui::Label::new(RichText::new(config["messages"]["quest_name"].as_str().unwrap_or("Explore a new quest")).size(18.).strong()).truncate());
            if let Some(expires)=config["expires_at"].as_str(){ui.label(RichText::new(format!("Ends {}",expires.get(..10).unwrap_or(expires))).size(11.).color(MUTED));}
            ui.add_space(8.);
            let tasks=config["task_config_v2"]["tasks"].as_object().or_else(||config["task_config"]["tasks"].as_object());
            if let Some(tasks)=tasks{for(name,task)in tasks.iter().take(5){
                let target=task["target"].as_f64().unwrap_or(1.).max(1.);let progress=quest["user_status"]["progress"][name]["value"].as_f64().or_else(||quest["user_status"]["progress"][name].as_f64()).unwrap_or(0.);
                ui.label(RichText::new(task["messages"]["task_title"].as_str().unwrap_or(match name.as_str(){"PLAY_ON_DESKTOP"=>"Play on desktop","STREAM_ON_DESKTOP"=>"Stream to a friend","WATCH_VIDEO"|"WATCH_VIDEO_ON_MOBILE"=>"Watch the featured video","PLAY_ACTIVITY"=>"Play an activity",_=>"Complete this quest in Discord"})).size(12.));
                if !quest["user_status"].is_null(){ui.add(egui::ProgressBar::new((progress/target).clamp(0.,1.)as f32).desired_height(6.).fill(self.accent()));}
            }}
            if let Some(rewards)=config["rewards_config"]["rewards"].as_array(){for reward in rewards.iter().take(4){ui.horizontal(|ui|{if let Some(url)=reward["asset"].as_str().and_then(|asset|quest_asset(quest["id"].as_str().unwrap_or(""),asset,None)){let(rect,_)=ui.allocate_exact_size(Vec2::splat(36.),egui::Sense::hover());crate::identity::paint_art(ui,&mut self.images,rect,Some(url),6);}ui.label(RichText::new(reward["messages"]["name"].as_str().unwrap_or("Quest reward")).size(12.).color(Color32::from_rgb(185,164,245)));});}}
            ui.add_space(10.);
            if self.preview{ui.add_enabled(false,egui::Button::new("Sample quest"));}
            else if quest["user_status"]["enrolled_at"].is_null(){if ui.add_sized([ui.available_width(),32.],primary("Accept Quest")).clicked(){if let Some(id)=quest["id"].as_str().filter(|id|community::snowflake(id)){self.mutate("quest-enroll",Method::POST,format!("/quests/{id}/enroll"),Some(json!({"location":11,"is_targeted":false})));}}}
            else if !quest["user_status"]["claimed_at"].is_null(){ui.label("✓ Reward claimed");}
            else{ui.hyperlink_to(if !quest["user_status"]["completed_at"].is_null(){"Claim reward in Discord ↗"}else{"Continue in Discord ↗"},"https://discord.com/quests");}
        });
    }
}
// Routes verified against Discord's publicly served client (2026-10-08).
fn safe_path(path:&str)->bool{!path.is_empty()&&path.len()<500&&path.split('/').all(|part|!part.is_empty()&&part!="."&&part!=".."&&part.bytes().all(|b|b.is_ascii_alphanumeric()||matches!(b,b'_'|b'-'|b'.')))}
fn collectible_asset(asset:&str)->Option<String>{
    if assets::public_url(asset){return Some(asset.into());}
    let asset=asset.trim_start_matches('/');if !safe_path(asset){return None;}
    let path=if asset.starts_with("assets/"){asset.to_owned()}else{format!("assets/collectibles/{asset}")};
    let url=format!("https://cdn.discordapp.com/{path}");assets::public_url(&url).then_some(url)
}
fn product_image(product:&Value,effects:&Value,animated:bool)->Option<String>{
    // Decoration presets are animated APNGs unless passthrough is off.
    let url=product_art(product,effects,animated)?;
    Some(if animated{url}else{url.replace("passthrough=true","passthrough=false")})
}
fn product_art(product:&Value,effects:&Value,animated:bool)->Option<String>{
    let preview=&product["preview_assets"];
    let candidates=if animated{[&preview["fg_animated"],&preview["fg_static"]]}else{[&preview["fg_static"],&preview["fg_animated"]]};
    if let Some(url)=candidates.into_iter().find_map(|v|v.as_str().and_then(collectible_asset)){return Some(url);}
    let items=product["items"].as_array()?;
    for item in items.iter().take(10){
        let direct=if animated{[&item["assets"]["animated_image_url"],&item["assets"]["static_image_url"],&item["thumbnailPreviewSrc"]]}else{[&item["assets"]["static_image_url"],&item["thumbnailPreviewSrc"],&item["assets"]["animated_image_url"]]};
        if let Some(url)=direct.into_iter().find_map(|v|v.as_str().and_then(collectible_asset)){return Some(url);}
        match item["type"].as_u64(){
            Some(0)=>{if let Some(url)=crate::identity::decoration(&User{avatar_decoration_data:Some(item.clone()),..Default::default()}){return Some(url);}},
            Some(1)=>{if let Some(config)=effects["profile_effect_configs"].as_array().or_else(||effects["profile_effects"].as_array()).into_iter().flatten().find(|e|e["id"].as_str().is_some()&&e["id"]==item["id"]){let config=config.get("config").unwrap_or(config);let candidates=if animated{[&config["thumbnailPreviewSrc"],&config["reducedMotionSrc"],&config["staticFrameSrc"]]}else{[&config["staticFrameSrc"],&config["reducedMotionSrc"],&config["thumbnailPreviewSrc"]]};if let Some(url)=candidates.into_iter().find_map(|v|v.as_str().and_then(collectible_asset)){return Some(url);}}},
            Some(2)=>{if let Some(url)=crate::identity::nameplate(&User{collectibles:Some(json!({"nameplate":item})),..Default::default()}){return Some(url);}},_=>{}
        }
    }None
}
fn quest_asset(id:&str,asset:&str,theme:Option<&str>)->Option<String>{
    if assets::public_url(asset){return Some(asset.into());}
    let path=asset.trim_start_matches('/');if !safe_path(path){return None;}
    let url=if path.contains('/'){format!("https://cdn.discordapp.com/{path}")}else{if !community::snowflake(id){return None;}format!("https://cdn.discordapp.com/quests/{id}/{}{path}",theme.map(|s|format!("{s}/")).unwrap_or_default())};
    assets::public_url(&url).then_some(url)
}
fn quest_hero(quest:&Value)->Option<String>{let id=quest["id"].as_str().unwrap_or("");let a=&quest["config"]["assets"];[&a["hero"],&a["hero_image"],&a["quest_bar_hero"],&a["game_tile_dark"]].into_iter().find_map(|v|v.as_str().and_then(|s|quest_asset(id,s,None)))}
fn matches_product(p:&Value,filter:&str)->bool{let kind=p["items"][0]["type"].as_u64();match filter{"Avatar decorations"=>kind==Some(0),"Profile effects"=>kind==Some(1),"Nameplates"=>kind==Some(2),"Bundles"=>kind==Some(1000),_=>true}}
fn empty_state(ui:&mut egui::Ui,title:&str,description:&str){ui.add_space(35.);ui.vertical_centered(|ui|{ui.heading(title);ui.weak(description);});ui.add_space(35.);}
fn store_hero(ui:&mut egui::Ui,images:&mut assets::Images,eyebrow:&str,title:&str,description:&str,a:Color32,b:Color32,quest:bool){
    egui::Frame::NONE.fill(a).corner_radius(14).inner_margin(24).show(ui,|ui|{let width=ui.available_width();ui.set_min_width(width);ui.set_min_height(142.);let rect=egui::Rect::from_min_size(ui.cursor().min,Vec2::new(width,142.));crate::identity::gradient(ui,rect.expand(24.),a,b);
        if quest{crate::identity::paint_art(ui,images,rect.expand(24.),Some("builtin://discord/quests-banner".into()),14);}
        else if width>490.{let art=egui::Rect::from_center_size(egui::pos2(rect.right()-145.,rect.center().y),Vec2::new(290.,116.));crate::identity::paint_art(ui,images,art,Some("builtin://discord/shop-banner".into()),10);}
        ui.scope(|ui|{ui.set_max_width(if !quest&&width>490.{width-310.}else{width});
            ui.label(RichText::new(eyebrow).size(10.).strong().color(Color32::from_rgb(211,197,247)));ui.add_space(10.);
            ui.add(egui::Label::new(RichText::new(title).size(27.).strong().color(Color32::WHITE)).wrap());ui.add_space(8.);
            ui.add(egui::Label::new(RichText::new(description).size(13.).color(Color32::from_rgb(232,225,245))).wrap());
        });
    });
}
fn sample_art(ui:&egui::Ui,rect:egui::Rect,kind:u64){
    let painter=ui.painter().with_clip_rect(rect);let c=rect.center();let r=rect.height().min(rect.width())*0.25;let mint=Color32::from_rgb(139,225,214);let purple=Color32::from_rgb(183,153,249);
    if kind==2{painter.rect_filled(egui::Rect::from_center_size(c,Vec2::new(rect.width()*0.8,44.)),12,Color32::from_rgb(67,87,118));painter.circle_filled(c-Vec2::new(rect.width()*0.26,0.),16.,purple);painter.text(c+Vec2::new(10.,0.),egui::Align2::CENTER_CENTER,"Your name",egui::FontId::proportional(15.),Color32::WHITE);}
    else{painter.circle_filled(c,r,Color32::from_black_alpha(40));painter.circle_stroke(c,r+9.,Stroke::new(4.0_f32,purple));for n in 0..5{let a=n as f32*std::f32::consts::TAU/5.;let p=c+Vec2::angled(a)*(r+13.);painter.circle_filled(p,4.,mint);}painter.text(c,egui::Align2::CENTER_CENTER,if kind==1{"✦"}else{"F"},egui::FontId::proportional(r),Color32::WHITE);}
}
fn sample_catalog()->Value {json!({"categories":[{"name":"Find your signature look","summary":"Sample collection · connect to browse Discord's live catalog","products":[{"name":"Starlight","summary":"A little cosmic energy","items":[{"type":0}]},{"name":"Aurora","summary":"A glow of your own","items":[{"type":1}]},{"name":"Daydream","summary":"Make your name stand out","items":[{"type":2}]}]}]})}
fn sample_quests()->Value{json!({"quests":[{"config":{"messages":{"quest_name":"Discover a new adventure","game_title":"SAMPLE GAME QUEST"},"task_config_v2":{"tasks":{"PLAY_ON_DESKTOP":{"target":900,"messages":{"task_title":"Play for 15 minutes"}}}},"rewards_config":{"rewards":[{"messages":{"name":"Example reward · preview only"}}]}}},{"config":{"messages":{"quest_name":"See what's coming next","game_title":"SAMPLE VIDEO QUEST"},"task_config_v2":{"tasks":{"WATCH_VIDEO":{"target":60,"messages":{"task_title":"Watch the featured trailer"}}}},"rewards_config":{"rewards":[{"messages":{"name":"Example reward · preview only"}}]}}}]})}

fn category_banner(category:&Value)->Option<String>{
    let assets=[&category["hero_banner_url"],&category["hero_banner_animated_url"],&category["assets"]["hero_static"],&category["assets"]["hero_animated"],&category["hero_banner_asset"]["animated"],&category["hero_banner_asset"]["static"],&category["catalog_banner_animated_url"],&category["catalog_banner_asset"]["animated"],&category["catalog_banner_asset"]["static"],&category["mobile_banner_url"],&category["banner_url"]];
    assets.into_iter().find_map(|v|v.as_str().and_then(collectible_asset))
}

#[cfg(test)]mod tests{use super::*;
    #[test]fn catalog_uses_each_items_actual_asset_and_effect_config(){
        let deco=json!({"items":[{"type":0,"asset":"a_12345678901234567890123456789012"}]});assert!(product_image(&deco,&Value::Null,true).unwrap().contains("avatar-decoration-presets/a_12345678901234567890123456789012.png"));
        let effect=json!({"items":[{"type":1,"id":"100"}]});let effects=json!({"profile_effect_configs":[{"id":"200","config":{"thumbnailPreviewSrc":"https://cdn.discordapp.com/wrong.png"}},{"id":"100","config":{"thumbnailPreviewSrc":"https://cdn.discordapp.com/right.gif","staticFrameSrc":"https://cdn.discordapp.com/right.png"}}]});assert!(product_image(&effect,&effects,true).unwrap().ends_with("right.gif"));assert!(product_image(&effect,&effects,false).unwrap().ends_with("right.png"));
        let plate=json!({"items":[{"type":2,"asset":"nameplates/forest/"}]});assert!(product_image(&plate,&Value::Null,true).unwrap().ends_with("nameplates/forest/img.png"));
        let product=json!({"preview_assets":{"fg_static":"nameplates/forest/preview.png"},"items":[]});assert!(product_image(&product,&Value::Null,false).unwrap().ends_with("nameplates/forest/preview.png"));
    }
    #[test]fn quest_art_resolves_filenames_and_root_paths_to_matching_quests(){
        let a=json!({"id":"123456789012345678","config":{"assets":{"hero":"game-hero.png"}}});let b=json!({"id":"223456789012345678","config":{"assets":{"hero":"quests/223456789012345678/other.png"}}});
        assert_eq!(quest_hero(&a).unwrap(),"https://cdn.discordapp.com/quests/123456789012345678/game-hero.png");assert_eq!(quest_hero(&b).unwrap(),"https://cdn.discordapp.com/quests/223456789012345678/other.png");assert!(quest_asset("123","../other.png",None).is_none());assert!(quest_asset("invalid","hero.png",None).is_none());
    }
}
