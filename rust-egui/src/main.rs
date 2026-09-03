use eframe::egui;
use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver, TryRecvError};

const ACCENT: egui::Color32 = egui::Color32::from_rgb(37, 99, 235);
const BG: egui::Color32 = egui::Color32::from_rgb(248, 249, 251);
const CARD: egui::Color32 = egui::Color32::WHITE;
const BORDER: egui::Color32 = egui::Color32::from_rgb(226, 232, 240);
const MUTED: egui::Color32 = egui::Color32::from_rgb(100, 116, 139);
const DANGER: egui::Color32 = egui::Color32::from_rgb(220, 38, 38);
const OK: egui::Color32 = egui::Color32::from_rgb(22, 163, 74);

#[derive(serde::Serialize, serde::Deserialize, Clone, Default)]
struct ModelInfo { #[serde(default, skip_serializing_if="Option::is_none")] name: Option<String> }
#[derive(serde::Serialize, serde::Deserialize, Clone, Default)]
struct Provider { #[serde(default)] npm: String, #[serde(default)] name: String, #[serde(default)] options: BTreeMap<String,String>, #[serde(default)] models: BTreeMap<String,ModelInfo> }
#[derive(serde::Serialize, serde::Deserialize, Default)]
struct Config { #[serde(default)] provider: BTreeMap<String, Provider> }

struct EditorApp {
    config: Config,
    config_path: PathBuf,
    selected: Option<String>,
    toast: Option<(String,bool,f64)>,
    search: String,
    fetch_state: FetchState,
    fetch_rx: Option<Receiver<(String, Result<Vec<String>,String>)>>,
    fetched: Vec<String>,
    fetch_pid: String,
    fetch_sel: HashSet<String>,
    fetch_err: Option<String>,
    renames: HashMap<String,String>,
    show_key: HashSet<String>,
    counter: u64,
}
enum FetchState{ Idle, Loading, Done } impl Default for FetchState{ fn default()->Self{FetchState::Idle} }

fn find_config()->PathBuf{
    let home=dirs::home_dir().unwrap_or(PathBuf::from("/"));
    let xdg=std::env::var("XDG_CONFIG_HOME").map(PathBuf::from).unwrap_or_else(|_| home.join(".config"));
    for p in [xdg.join("opencode/opencode.jsonc"), xdg.join("opencode/opencode.json"), home.join(".config/opencode/opencode.jsonc"), home.join(".config/opencode/opencode.json"), PathBuf::from("/home/lenovo/.config/opencode/opencode.jsonc")] {
        if p.exists(){ return p; }
    }
    xdg.join("opencode/opencode.jsonc")
}
fn load(p:&PathBuf)->(Config,Option<String>){
    match std::fs::read_to_string(p){
        Ok(raw)=> match strip(&raw){ Some(c)=> match serde_json::from_str(&c){ Ok(v)=>(v,None), Err(e)=>(Config::default(),Some(format!("Parse: {e}")))}, None=>(Config::default(),Some("JSONC invalid".into()))},
        Err(e)=>(Config::default(),Some(format!("Read: {e}")))
    }
}
fn save(p:&PathBuf,c:&Config)->Result<(),String>{
    if p.exists(){ let b=p.with_extension("bak"); std::fs::copy(p,&b).map_err(|e| format!("Backup: {e}"))?; }
    let j=serde_json::to_string_pretty(c).map_err(|e| format!("Ser: {e}"))?;
    std::fs::write(p, format!("{j}\n")).map_err(|e| format!("Write: {e}"))
}
fn strip(raw:&str)->Option<String>{
    let mut out=String::with_capacity(raw.len()); let mut ins=false; let mut esc=false; let mut line=false; let mut block=false; let mut chars=raw.chars().peekable();
    while let Some(c)=chars.next(){ if line{ if c=='\n'{line=false; out.push(c);} continue;} if block{ if c=='*' && chars.peek()==Some(&'/'){chars.next(); block=false;} continue;} if ins{ out.push(c); if esc{esc=false;} else if c=='\\'{esc=true;} else if c=='"'{ins=false;} continue;} match c{ '"' =>{ins=true; out.push(c);} '/' if chars.peek()==Some(&'/')=>{chars.next(); line=true;} '/' if chars.peek()==Some(&'*')=>{chars.next(); block=true;} _=> out.push(c), }}
    if ins||block{None}else{Some(out)}
}
fn fetch_models(base:&str,key:&str)->Result<Vec<String>,String>{
    let url=format!("{}/models", base.trim_end_matches('/'));
    let mut req=ureq::Agent::new().get(&url);
    if !key.is_empty(){ req=req.set("Authorization",&format!("Bearer {key}")); }
    let body=req.call().map_err(|e| format!("{e}"))?.into_string().map_err(|e| format!("{e}"))?;
    let v: serde_json::Value=serde_json::from_str(&body).map_err(|e| format!("{e}"))?;
    let arr=if let Some(a)=v.as_array(){a.clone()} else if let Some(d)=v.get("data").and_then(|x| x.as_array()){d.clone()} else if let Some(m)=v.get("models").and_then(|x| x.as_array()){m.clone()} else{return Err("No models".into());};
    Ok(arr.into_iter().filter_map(|m| if let Some(s)=m.as_str(){Some(s.to_string())} else if let Some(id)=m.get("id").and_then(|x| x.as_str()){Some(id.to_string())} else {None}).collect())
}

impl EditorApp{
    fn new(cc:&eframe::CreationContext<'_>)->Self{
        let mut s=(*cc.egui_ctx.style()).clone();
        s.visuals=egui::Visuals::light();
        s.visuals.panel_fill=BG;
        s.visuals.window_fill=CARD;
        s.visuals.window_stroke=egui::Stroke::new(1.0_f32, BORDER);
        s.visuals.window_corner_radius=egui::CornerRadius::same(12);
        s.visuals.window_shadow=egui::epaint::Shadow::NONE;
        s.visuals.widgets.noninteractive.bg_fill=egui::Color32::TRANSPARENT;
        s.visuals.widgets.inactive.bg_fill=CARD;
        s.visuals.widgets.inactive.bg_stroke=egui::Stroke::new(1.0_f32, BORDER);
        s.visuals.widgets.inactive.corner_radius=egui::CornerRadius::same(8);
        s.visuals.widgets.hovered.bg_fill=egui::Color32::from_rgb(241,245,249);
        s.visuals.widgets.hovered.bg_stroke=egui::Stroke::new(1.0_f32, ACCENT);
        s.visuals.widgets.active.bg_fill=egui::Color32::from_rgb(219,234,254);
        s.visuals.selection.bg_fill=ACCENT;
        s.spacing.item_spacing=egui::vec2(8.0,8.0);
        s.spacing.button_padding=egui::vec2(10.0,6.0);
        s.spacing.interact_size.y=30.0;
        cc.egui_ctx.set_style(s);
        cc.egui_ctx.set_visuals(egui::Visuals::light());
        let path=find_config();
        let (cfg,err)=load(&path);
        let sel=cfg.provider.keys().next().cloned();
        let mut c=0u64; for p in cfg.provider.values(){ for k in p.models.keys(){ if let Some(n)=k.split('-').last().and_then(|s| s.parse::<u64>().ok()){ c=c.max(n);} } }
        let mut app=Self{ config:cfg, config_path:path, selected:sel, toast:None, search:String::new(), fetch_state:FetchState::Idle, fetch_rx:None, fetched:Vec::new(), fetch_pid:String::new(), fetch_sel:Default::default(), fetch_err:None, renames:Default::default(), show_key:Default::default(), counter:c };
        if let Some(e)=err{ app.toast=Some((e,true,0.0)); }
        app
    }
    fn toast(&mut self, ctx:&egui::Context, msg:&str, err:bool){ self.toast=Some((msg.to_string(),err,ctx.input(|i| i.time))); }
    fn primary(&self, ui:&mut egui::Ui, t:&str)->egui::Response{
        ui.add(egui::Button::new(egui::RichText::new(t).color(egui::Color32::WHITE).strong()).fill(ACCENT).stroke(egui::Stroke::NONE).corner_radius(8).min_size(egui::vec2(0.0,32.0)))
    }
    fn ghost(&self, ui:&mut egui::Ui, t:&str)->egui::Response{
        ui.add(egui::Button::new(egui::RichText::new(t).size(13.0)).fill(CARD).stroke(egui::Stroke::new(1.0_f32,BORDER)).corner_radius(8).min_size(egui::vec2(0.0,32.0)))
    }
}

impl eframe::App for EditorApp{
    fn update(&mut self, ctx:&egui::Context, _:&mut eframe::Frame){
        let now=ctx.input(|i| i.time);
        if let Some((_,_,t))=self.toast.clone(){ if now-t>3.5{ self.toast=None; } }
        if let Some(rx)=&self.fetch_rx{
            match rx.try_recv(){
                Ok((pid,Ok(list)))=>{ self.fetched=list; self.fetch_pid=pid; self.fetch_sel.clear(); self.fetch_state=FetchState::Done; self.fetch_rx=None; }
                Ok((_,Err(e)))=>{ self.fetch_state=FetchState::Idle; self.fetch_rx=None; self.fetch_err=Some(e.clone()); self.toast(ctx,&format!("Fetch gagal: {e}"),true); }
                Err(TryRecvError::Empty)=>{}
                Err(TryRecvError::Disconnected)=>{ self.fetch_rx=None; self.fetch_state=FetchState::Idle; }
            }
        }

        egui::TopBottomPanel::top("topbar").frame(
            egui::Frame::new().fill(CARD).stroke(egui::Stroke::new(1.0_f32,BORDER)).inner_margin(egui::Margin::symmetric(16,10))
        ).show(ctx, |ui|{
            ui.horizontal(|ui|{
                ui.label(egui::RichText::new("⬢").size(18.0).color(ACCENT));
                ui.label(egui::RichText::new("openconfig").strong().size(16.0));
                ui.add_space(8.0);
                ui.label(egui::RichText::new(self.config_path.display().to_string()).monospace().size(11.0).color(MUTED));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui|{
                    if self.primary(ui,"  Save  ").clicked(){
                        match save(&self.config_path,&self.config){ Ok(())=> self.toast(ctx,"Tersimpan ✓",false), Err(e)=> self.toast(ctx,&format!("Save: {e}"),true) }
                    }
                    if self.ghost(ui,"Reload").clicked(){
                        let (c,e)=load(&self.config_path); self.config=c;
                        if let Some(x)=e{ self.toast(ctx,&x,true);} else{ self.toast(ctx,"Reloaded",false); }
                        if self.selected.is_none(){ self.selected=self.config.provider.keys().next().cloned(); }
                    }
                });
            });
            if let Some((m,err,_))=self.toast.clone(){
                ui.add_space(4.0);
                ui.label(egui::RichText::new(m).size(12.0).color(if err{DANGER}else{OK}));
            }
        });

        egui::TopBottomPanel::bottom("foot").frame(
            egui::Frame::new().fill(BG).inner_margin(egui::Margin::symmetric(16,6))
        ).show(ctx, |ui|{
            ui.horizontal(|ui|{
                ui.label(egui::RichText::new(format!("{} providers", self.config.provider.len())).size(11.0).color(MUTED));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui|{
                    ui.label(egui::RichText::new("provider & model editor — light").size(11.0).color(MUTED));
                });
            });
        });

        egui::SidePanel::left("sidebar")
            .frame(egui::Frame::new().fill(CARD).stroke(egui::Stroke::new(1.0_f32,BORDER)).inner_margin(egui::Margin::symmetric(12,12)))
            .resizable(false)
            .default_width(260.0)
            .show(ctx, |ui|{
                ui.horizontal(|ui|{
                    ui.label(egui::RichText::new("Providers").strong().size(13.0));
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui|{
                        if self.ghost(ui,"+").clicked(){
                            let mut n=self.config.provider.len();
                            let mut pid=format!("provider-{n}");
                            while self.config.provider.contains_key(&pid){ n+=1; pid=format!("provider-{n}"); }
                            self.config.provider.insert(pid.clone(), Provider{npm:"@ai-sdk/openai-compatible".into(), name:"New Provider".into(), ..Default::default()});
                            self.selected=Some(pid);
                        }
                    });
                });
                ui.add_space(6.0);
                ui.add(egui::TextEdit::singleline(&mut self.search).hint_text("Search provider…").desired_width(f32::INFINITY));
                ui.add_space(8.0);
                ui.separator();
                ui.add_space(4.0);
                egui::ScrollArea::vertical().show(ui, |ui|{
                    let q=self.search.to_lowercase();
                    for pid in self.config.provider.keys().cloned().collect::<Vec<_>>(){
                        let prov=self.config.provider.get(&pid).unwrap();
                        if !q.is_empty() && !pid.to_lowercase().contains(&q) && !prov.name.to_lowercase().contains(&q){ continue; }
                        let sel=self.selected.as_deref()==Some(&pid);
                        let frame=if sel{
                            egui::Frame::new().fill(egui::Color32::from_rgb(239,246,255)).stroke(egui::Stroke::new(1.0_f32, ACCENT)).corner_radius(8).inner_margin(egui::Margin::symmetric(10,8))
                        } else {
                            egui::Frame::new().fill(CARD).stroke(egui::Stroke::new(1.0_f32,BORDER)).corner_radius(8).inner_margin(egui::Margin::symmetric(10,8))
                        };
                        let resp=frame.show(ui, |ui|{
                            ui.set_width(ui.available_width());
                            ui.label(egui::RichText::new(&prov.name).strong().size(13.0).color(if sel{ACCENT}else{egui::Color32::from_gray(30)}));
                            ui.label(egui::RichText::new(&pid).monospace().size(11.0).color(MUTED));
                            ui.label(egui::RichText::new(format!("{} models", prov.models.len())).size(11.0).color(MUTED));
                        }).response;
                        if resp.interact(egui::Sense::click()).clicked(){
                            self.selected=Some(pid.clone());
                        }
                    }
                });
            });

        egui::CentralPanel::default()
            .frame(egui::Frame::new().fill(BG).inner_margin(egui::Margin::symmetric(16,12)))
            .show(ctx, |ui|{
                let Some(pid)=self.selected.clone() else{
                    ui.vertical_centered(|ui|{ ui.add_space(80.0); ui.label(egui::RichText::new(" Pilih provider di sidebar ").size(14.0).color(MUTED)); });
                    return;
                };
                let mut prov=self.config.provider.get(&pid).cloned().unwrap_or_default();
                egui::ScrollArea::vertical().show(ui, |ui|{
                    egui::Frame::new().fill(CARD).stroke(egui::Stroke::new(1.0_f32,BORDER)).corner_radius(12).inner_margin(egui::Margin::same(16)).show(ui, |ui|{
                        ui.horizontal(|ui|{
                            ui.label(egui::RichText::new("Provider").strong().size(13.0).color(MUTED));
                            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui|{
                                if ui.add(egui::Button::new(egui::RichText::new("✕ Hapus").size(12.0).color(DANGER)).fill(egui::Color32::from_rgb(254,242,242)).stroke(egui::Stroke::new(1.0_f32, egui::Color32::from_rgb(254,226,226))).corner_radius(6)).clicked(){
                                    self.config.provider.remove(&pid);
                                    self.selected=self.config.provider.keys().next().cloned();
                                    self.toast(ctx,"Provider dihapus (belum save)",false);
                                    return;
                                }
                            });
                        });
                        ui.add_space(6.0);
                        egui::Grid::new("provfields").num_columns(2).spacing([12.0,8.0]).show(ui, |ui|{
                            ui.label(egui::RichText::new("Provider ID").weak().size(12.0));
                            let mut edit=self.renames.get(&pid).cloned().unwrap_or(pid.clone());
                            if ui.add(egui::TextEdit::singleline(&mut edit).font(egui::TextStyle::Monospace).desired_width(360.0)).changed(){
                                self.renames.insert(pid.clone(), edit);
                            }
                            ui.end_row();
                            ui.label(egui::RichText::new("Display Name").weak().size(12.0));
                            ui.add(egui::TextEdit::singleline(&mut prov.name).desired_width(360.0));
                            ui.end_row();
                            ui.label(egui::RichText::new("npm").weak().size(12.0));
                            ui.add(egui::TextEdit::singleline(&mut prov.npm).font(egui::TextStyle::Monospace).desired_width(360.0));
                            ui.end_row();
                            ui.label(egui::RichText::new("baseURL").weak().size(12.0));
                            let b=prov.options.entry("baseURL".into()).or_default();
                            ui.add(egui::TextEdit::singleline(b).font(egui::TextStyle::Monospace).desired_width(360.0).hint_text("http://localhost:xxxx/v1"));
                            ui.end_row();
                            ui.label(egui::RichText::new("apiKey").weak().size(12.0));
                            let k=prov.options.entry("apiKey".into()).or_default();
                            ui.horizontal(|ui|{
                                let show=self.show_key.contains(&pid);
                                let te=egui::TextEdit::singleline(k).font(egui::TextStyle::Monospace).desired_width(320.0).password(!show);
                                ui.add(te);
                                if ui.small_button(if show{"🙈"} else {"👁"}).clicked(){
                                    if show{ self.show_key.remove(&pid);} else{ self.show_key.insert(pid.clone()); }
                                }
                            });
                            ui.end_row();
                        });
                    });

                    ui.add_space(12.0);

                    egui::Frame::new().fill(CARD).stroke(egui::Stroke::new(1.0_f32,BORDER)).corner_radius(12).inner_margin(egui::Margin::same(16)).show(ui, |ui|{
                        ui.horizontal(|ui|{
                            ui.label(egui::RichText::new("Models").strong().size(13.0));
                            ui.label(egui::RichText::new(format!("{} total", prov.models.len())).weak().size(12.0));
                            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui|{
                                if self.primary(ui,"+ Model").clicked(){
                                    self.counter+=1;
                                    prov.models.insert(format!("model-{}", self.counter), ModelInfo::default());
                                }
                                let base_empty=prov.options.get("baseURL").map(|s| s.trim().is_empty()).unwrap_or(true);
                                let btn=egui::Button::new("↻ Fetch").fill(CARD).stroke(egui::Stroke::new(1.0_f32,BORDER)).corner_radius(8);
                                if ui.add_enabled(!base_empty, btn).clicked(){
                                    let base=prov.options.get("baseURL").cloned().unwrap_or_default();
                                    let key=prov.options.get("apiKey").cloned().unwrap_or_default();
                                    let pid_send=pid.clone();
                                    let (tx,rx)=mpsc::channel();
                                    std::thread::spawn(move||{ let r=fetch_models(&base,&key); let _=tx.send((pid_send,r)); });
                                    self.fetch_rx=Some(rx);
                                    self.fetch_state=FetchState::Loading;
                                    self.fetch_pid=pid.clone();
                                }
                            });
                        });
                        ui.add_space(8.0);
                        ui.separator();
                        ui.add_space(6.0);
                        if prov.models.is_empty(){
                            ui.label(egui::RichText::new("Belum ada model — tambah manual atau Fetch").size(13.0).color(MUTED));
                        }
                        let keys: Vec<String>=prov.models.keys().cloned().collect();
                        let mut remove: Option<String>=None;
                        for k in keys{
                            let v=prov.models.get(&k).cloned().unwrap_or_default();
                            egui::Frame::new().fill(egui::Color32::from_rgb(249,250,251)).stroke(egui::Stroke::new(1.0_f32,BORDER)).corner_radius(8).inner_margin(egui::Margin::symmetric(10,6)).show(ui, |ui|{
                                ui.horizontal(|ui|{
                                    let mut nk=k.clone();
                                    let mut nn=v.name.clone().unwrap_or_default();
                                    ui.add_sized([260.0,28.0], egui::TextEdit::singleline(&mut nk).font(egui::TextStyle::Monospace).hint_text("model id"));
                                    ui.add_sized([200.0,28.0], egui::TextEdit::singleline(&mut nn).hint_text("display name"));
                                    if ui.add(egui::Button::new("✕").fill(egui::Color32::from_rgb(254,242,242)).stroke(egui::Stroke::new(1.0_f32,egui::Color32::from_rgb(254,226,226))).corner_radius(6).min_size(egui::vec2(28.0,24.0))).clicked(){
                                        remove=Some(k.clone());
                                    }
                                    if nk!=k{
                                        if let Some(info)=prov.models.remove(&k){
                                            prov.models.insert(nk.clone(), info);
                                        }
                                    } else if nn!=v.name.clone().unwrap_or_default(){
                                        if let Some(entry)=prov.models.get_mut(&k){
                                            entry.name=if nn.trim().is_empty(){None}else{Some(nn.trim().to_string())};
                                        }
                                    }
                                });
                            });
                        }
                        if let Some(k)=remove{ prov.models.remove(&k); }
                    });

                    ui.add_space(12.0);
                    let pretty=serde_json::to_string_pretty(&{
                        let mut m=BTreeMap::new();
                        m.insert(pid.clone(), &prov);
                        serde_json::json!({"provider": m})
                    }).unwrap_or_default();
                    egui::CollapsingHeader::new(egui::RichText::new("Preview JSON (provider ini)").size(12.0).color(MUTED)).default_open(false).show(ui, |ui|{
                        egui::ScrollArea::vertical().max_height(220.0).show(ui, |ui|{
                            ui.add(egui::TextEdit::multiline(&mut pretty.clone()).font(egui::TextStyle::Monospace).desired_width(f32::INFINITY).desired_rows(8));
                        });
                    });

                    let final_key=self.renames.get(&pid).cloned().unwrap_or(pid.clone());
                    if final_key.trim().is_empty(){
                        // keep original
                    } else if final_key!=pid{
                        self.config.provider.remove(&pid);
                        self.config.provider.insert(final_key.clone(), prov);
                        self.selected=Some(final_key);
                        for (k,v) in self.renames.clone(){ if k==pid{ self.renames.remove(&k); } let _=v;}
                    } else {
                        self.config.provider.insert(pid.clone(), prov);
                    }
                });
            });

        match self.fetch_state{
            FetchState::Loading=>{
                egui::Window::new("Fetch models").collapsible(false).resizable(false).anchor(egui::Align2::CENTER_CENTER, egui::vec2(0.0,0.0)).show(ctx, |ui|{
                    ui.horizontal(|ui|{ ui.spinner(); ui.label("Mengambil models…"); });
                });
            }
            FetchState::Done=>{
                let existing: HashSet<String>=self.config.provider.get(&self.fetch_pid).map(|p| p.models.keys().cloned().collect()).unwrap_or_default();
                let avail: Vec<String>=self.fetched.iter().filter(|m| !existing.contains(*m)).cloned().collect();
                egui::Window::new("Pilih model").collapsible(false).resizable(true).default_width(460.0).show(ctx, |ui|{
                    ui.label(egui::RichText::new(format!("Dari {}", self.fetch_pid)).weak().size(12.0));
                    ui.separator();
                    ui.horizontal(|ui|{
                        let all=!avail.is_empty() && self.fetch_sel.len()==avail.len();
                        let mut chk=all;
                        if ui.checkbox(&mut chk,"Pilih semua").changed(){
                            if chk{ self.fetch_sel.extend(avail.iter().cloned()); } else{ self.fetch_sel.clear(); }
                        }
                        if ui.button("Kosongkan").clicked(){ self.fetch_sel.clear(); }
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui|{
                            ui.label(egui::RichText::new(format!("{} dipilih", self.fetch_sel.len())).size(12.0).color(MUTED));
                        });
                    });
                    egui::ScrollArea::vertical().max_height(340.0).show(ui, |ui|{
                        for m in avail.iter(){
                            let mut sel=self.fetch_sel.contains(m);
                            if ui.checkbox(&mut sel, egui::RichText::new(m).monospace()).changed(){
                                if sel{ self.fetch_sel.insert(m.clone()); } else{ self.fetch_sel.remove(m); }
                            }
                        }
                        if avail.is_empty(){ ui.label(egui::RichText::new("Semua model sudah ada").size(12.0).color(MUTED)); }
                    });
                    ui.separator();
                    ui.horizontal(|ui|{
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui|{
                            if self.primary(ui,"Add dipilih").clicked(){
                                let picked: Vec<String>=self.fetch_sel.iter().cloned().collect();
                                if let Some(p)=self.config.provider.get_mut(&self.fetch_pid){
                                    for m in picked.iter(){
                                        let n=m.rsplit('/').next().unwrap_or(m).to_string();
                                        p.models.entry(m.clone()).or_insert_with(|| ModelInfo{name:Some(n)});
                                    }
                                }
                                self.toast(ctx,&format!("{} model ditambah (belum save)", picked.len()),false);
                                self.fetch_state=FetchState::Idle;
                            }
                            if self.ghost(ui,"Batal").clicked(){ self.fetch_state=FetchState::Idle; }
                        });
                    });
                });
            }
            FetchState::Idle=>{}
        }
        if let Some(e)=self.fetch_err.clone(){
            egui::Window::new("Fetch gagal").collapsible(false).anchor(egui::Align2::CENTER_CENTER, egui::vec2(0.0,0.0)).show(ctx, |ui|{
                ui.colored_label(DANGER,&e);
                if self.ghost(ui,"Tutup").clicked(){ self.fetch_err=None; }
            });
        }
    }
}

fn main()->eframe::Result<()>{
    if std::env::var("LIBGL_ALWAYS_SOFTWARE").is_err(){
        std::env::set_var("LIBGL_ALWAYS_SOFTWARE","1");
    }
    if std::env::var("GALLIUM_DRIVER").is_err(){
        std::env::set_var("GALLIUM_DRIVER","llvmpipe");
    }
    let opts=eframe::NativeOptions{
        viewport: egui::ViewportBuilder::default().with_inner_size([1150.0,820.0]).with_min_inner_size([900.0,600.0]).with_title("openconfig"),
        renderer: eframe::Renderer::Glow,
        vsync: true,
        ..Default::default()
    };
    eframe::run_native("openconfig", opts, Box::new(|cc| Ok(Box::new(EditorApp::new(cc)))))
}
