import { useEffect, useRef, useState, type ChangeEvent, type DragEvent, type ReactNode } from "react";
import { invoke } from "@tauri-apps/api/core";
import { QRCodeSVG } from "qrcode.react";
import { AnimatePresence, motion, useReducedMotion } from "framer-motion";
import {
 Activity, ArrowLeftRight, ArrowUpRight, Check, CheckCheck, ChevronDown, ChevronRight,
 CircleHelp, Clipboard, Clock3, Copy, File, FileImage, FileText, Files, FolderOpen,
 LockKeyhole, Mail, Menu, MessageCircle, Monitor, Moon, PanelLeftClose, Plus, QrCode,
 Send, Settings2, ShieldCheck, Smartphone, Sun, Trash2, Upload, Wifi, X, RefreshCw, KeyRound
} from "lucide-react";
import { clampSupportMessage, formatBytes, SUPPORT_MESSAGE_LIMIT, SUPPORT_SCREENSHOT_LIMIT, validateScreenshot } from "./lib/validation";
import { loadPreferences, savePreferences } from "./lib/preferences";

type Page="home"|"send"|"devices"|"clipboard"|"activity"|"settings"|"help";
type Theme="light"|"dark"|"system";
type Attachment={id:string;file:File;url:string};
type PeerTransportStatus={running:boolean;endpointId:string|null;identityPersisted:boolean};
type PeerProbe={connected:boolean;remoteEndpointId:string;protocolVersion:number;message:string};
type PairingInvite={endpointId:string;code:string;expiresInSeconds:number};
type TrustedDevice={endpointId:string;pairedAtUnix:number};
type Transfer={id:string;name:string;size:string;time:string;state:"Complete"|"Waiting"|"Failed";direction:"sent"|"received"};
const navPrimary:{id:Page;label:string;icon:typeof Monitor}[]=[
 {id:"home",label:"Overview",icon:Monitor},{id:"send",label:"Send",icon:ArrowUpRight},
 {id:"devices",label:"Devices",icon:Smartphone},{id:"clipboard",label:"Clipboard",icon:Clipboard},
 {id:"activity",label:"Activity",icon:Activity}
];
const initialTransfers:Transfer[]=[
 {id:"t1",name:"Weekend photos.zip",size:"248 MB",time:"Today · 10:42",state:"Complete",direction:"sent"},
 {id:"t2",name:"Project notes.pdf",size:"1.8 MB",time:"Today · 09:16",state:"Complete",direction:"received"},
 {id:"t3",name:"Design references",size:"12.4 MB",time:"Yesterday · 18:30",state:"Complete",direction:"sent"}
];
const initialClipboard=[
 {id:"c1",type:"Link",value:"https://linear.app",note:"Copied from Windows PC",time:"8 min ago"},
 {id:"c2",type:"Text",value:"The details are in the shared folder. I'll send the final version this afternoon.",note:"Copied from Android phone",time:"Yesterday"}
];
function cx(...values:Array<string|false|undefined>){return values.filter(Boolean).join(" ");}

function App(){
 const prefersReduced=useReducedMotion();
 const [preferences]=useState(loadPreferences);
 const [page,setPage]=useState<Page>("home");
 const [theme,setTheme]=useState<Theme>(preferences.theme);
 const [dark,setDark]=useState(false);
 const [reducedMotion,setReducedMotion]=useState(preferences.reducedMotion || Boolean(prefersReduced));
 const [sidebarOpen,setSidebarOpen]=useState(!preferences.sidebarCollapsed);
 const [transfers,setTransfers]=useState(initialTransfers);
 const [clipboardItems,setClipboardItems]=useState(initialClipboard);
 const [clipboardSync,setClipboardSync]=useState(preferences.clipboardSync);
 const [message,setMessage]=useState("");
 const [attachments,setAttachments]=useState<Attachment[]>([]);
 const [supportStatus,setSupportStatus]=useState("");
 const [dragging,setDragging]=useState(false);
 const [toast,setToast]=useState("");
 const [sendText,setSendText]=useState("");
 const [faqOpen,setFaqOpen]=useState<number|null>(0);

 const [settingsSaved,setSettingsSaved]=useState(false);
 const [peerStatus,setPeerStatus]=useState<PeerTransportStatus>({running:false,endpointId:null,identityPersisted:false});
 const [peerBusy,setPeerBusy]=useState(false);
 const [peerError,setPeerError]=useState("");
 const [peerTargetId,setPeerTargetId]=useState("");
 const [peerProbe,setPeerProbe]=useState<PeerProbe|null>(null);
 const [peerProbeBusy,setPeerProbeBusy]=useState(false);
 const [peerProbeError,setPeerProbeError]=useState("");
 const [pairingInvite,setPairingInvite]=useState<PairingInvite|null>(null);
 const [pairingInviteBusy,setPairingInviteBusy]=useState(false);
 const [trustedDevices,setTrustedDevices]=useState<TrustedDevice[]>([]);
 const [pairingDeviceId,setPairingDeviceId]=useState("");
 const [pairingCode,setPairingCode]=useState("");
 const [pairingBusy,setPairingBusy]=useState(false);
 const [pairingError,setPairingError]=useState("");
 const [trustedBusy,setTrustedBusy]=useState(false);
 useEffect(()=>{
  savePreferences({theme,reducedMotion,clipboardSync,sidebarCollapsed:!sidebarOpen});
 },[theme,reducedMotion,clipboardSync,sidebarOpen]);
 useEffect(()=>{invoke<PeerTransportStatus>("get_peer_transport_status").then(setPeerStatus).catch(()=>{});},[]);
 useEffect(()=>{invoke<TrustedDevice[]>("get_trusted_devices").then(setTrustedDevices).catch(()=>{});},[]);
 const startPeerTransport=async()=>{
  setPeerBusy(true);setPeerError("");
  try{const status=await invoke<PeerTransportStatus>("start_peer_transport");setPeerStatus(status);toastMessage("Peer endpoint started. Pairing and file transfer are still in development.");}
  catch(error){setPeerError(typeof error==="string"?error:"Native peer networking is available inside the installed Bridge app.");}
  finally{setPeerBusy(false);}
 };
 const stopPeerTransport=async()=>{
  setPeerBusy(true);setPeerError("");
  try{const status=await invoke<PeerTransportStatus>("stop_peer_transport");setPeerStatus(status);setTrustedDevices([]);setPairingInvite(null);setPeerProbe(null);toastMessage("Peer endpoint stopped. Session pairings were cleared.");}
  catch(error){setPeerError(typeof error==="string"?error:"Could not stop the native peer endpoint.");}
  finally{setPeerBusy(false);}
 };
 const probePeerEndpoint=async()=>{
  if(!peerTargetId.trim()){setPeerProbeError("Enter the other device's endpoint ID.");return;}
  setPeerProbeBusy(true);setPeerProbeError("");setPeerProbe(null);
  try{
   const result=await invoke<PeerProbe>("probe_peer_endpoint",{remoteEndpointId:peerTargetId.trim()});
   setPeerProbe(result);
   toastMessage("Encrypted peer handshake completed.");
  }catch(error){
   setPeerProbeError(typeof error==="string"?error:"Could not connect to that peer.");
  }finally{setPeerProbeBusy(false);}
 };
 const createPairingInvite=async()=>{
  setPairingInviteBusy(true);setPairingError("");
  try{
   const invite=await invoke<PairingInvite>("create_pairing_invite");
   setPairingInvite(invite);
   toastMessage("One-time pairing code created. It expires in 3 minutes.");
  }catch(error){setPairingError(typeof error==="string"?error:"Could not create a pairing invitation.");}
  finally{setPairingInviteBusy(false);}
 };
 const refreshTrustedDevices=async()=>{
  setTrustedBusy(true);
  try{setTrustedDevices(await invoke<TrustedDevice[]>("get_trusted_devices"));}
  catch(error){setPairingError(typeof error==="string"?error:"Could not refresh paired devices.");}
  finally{setTrustedBusy(false);}
 };
 const pairWithDevice=async()=>{
  if(!pairingDeviceId.trim()||pairingCode.length!==6){setPairingError("Enter the other device ID and its six-digit invite code.");return;}
  setPairingBusy(true);setPairingError("");
  try{
   const peer=await invoke<TrustedDevice>("pair_with_endpoint",{remoteEndpointId:pairingDeviceId.trim(),pairingCode});
   setTrustedDevices(current=>[peer,...current.filter(item=>item.endpointId!==peer.endpointId)]);
   setPairingCode("");
   toastMessage("Devices paired for this session.");
  }catch(error){setPairingError(typeof error==="string"?error:"Pairing could not be completed.");}
  finally{setPairingBusy(false);}
 };
 const removeTrustedDevice=async(endpointId:string)=>{
  try{
   await invoke<boolean>("remove_trusted_device",{endpointId});
   setTrustedDevices(current=>current.filter(item=>item.endpointId!==endpointId));
   toastMessage("Device removed from this session.");
  }catch(error){setPairingError(typeof error==="string"?error:"Could not remove the device.");}
 };
 const copyPairingCode=async()=>{
  if(!pairingInvite)return;
  try{await navigator.clipboard.writeText(pairingInvite.code);toastMessage("Pairing code copied.");}
  catch{toastMessage("Clipboard access is unavailable in this context.");}
 };
 const copyLocalPeerId=async()=>{
  if(!peerStatus.endpointId)return;
  try{await navigator.clipboard.writeText(peerStatus.endpointId);toastMessage("Device ID copied.");}
  catch{toastMessage("Clipboard access is unavailable in this context.");}
 };
 useEffect(()=>{
  if(theme==="system"){
   const media=window.matchMedia("(prefers-color-scheme: dark)");
   const sync=()=>setDark(media.matches);
   sync();
   media.addEventListener("change",sync);
   return ()=>media.removeEventListener("change",sync);
  }
  setDark(theme==="dark");
 },[theme]);
 const fileInput=useRef<HTMLInputElement>(null);
 const supportInput=useRef<HTMLInputElement>(null);
 const transition=reducedMotion?{duration:0}:{duration:.34,ease:[.22,1,.36,1] as const};
 const toastMessage=(value:string)=>{setToast(value);window.setTimeout(()=>setToast(""),2600);};
 const navigate=(next:Page)=>setPage(next);
 const addFiles=(files:FileList|File[])=>{
  const accepted=Array.from(files);if(!accepted.length)return;
  const entries:Transfer[]=accepted.map((file,index)=>({id:`local-${Date.now()}-${index}`,name:file.name,size:formatBytes(file.size),time:"Just now",state:"Waiting",direction:"sent"}));
  setTransfers(current=>[...entries,...current]);toastMessage(`${accepted.length} file${accepted.length>1?"s":""} added to the local demo queue`);navigate("send");
 };
 const onDrop=(event:DragEvent<HTMLDivElement>)=>{event.preventDefault();setDragging(false);addFiles(event.dataTransfer.files);};
 const handleScreenshots=(event:ChangeEvent<HTMLInputElement>)=>{
  const selected=Array.from(event.target.files??[]);const valid:Attachment[]=[];const errors:string[]=[];
  for(const file of selected){
   const error=validateScreenshot(file);if(error){errors.push(`${file.name}: ${error}`);continue;}
   if(attachments.length+valid.length>=SUPPORT_SCREENSHOT_LIMIT){errors.push("You can attach up to 5 screenshots.");break;}
   valid.push({id:`${Date.now()}-${valid.length}`,file,url:URL.createObjectURL(file)});
  }
  setAttachments(current=>[...current,...valid]);if(errors.length)toastMessage(errors[0]);event.target.value="";
 };
 const removeAttachment=(id:string)=>setAttachments(current=>{
  const item=current.find(entry=>entry.id===id);if(item)URL.revokeObjectURL(item.url);return current.filter(entry=>entry.id!==id);
 });
 const openSupportEmail=()=>{
  if(!message.trim()){setSupportStatus("Add a short description before opening the email.");return;}
  const subject=encodeURIComponent("Bridge support request");
  const body=encodeURIComponent(`${message.trim()}\n\nScreenshots to attach manually: ${attachments.map(item=>item.file.name).join(", ")||"None"}\n\nBridge version: 0.1.0`);
  window.location.href=`mailto:linka.comp@gmail.com?subject=${subject}&body=${body}`;
  setSupportStatus("Your email app should open. Add the selected screenshots before sending.");
 };
 const pageTitle:Record<Page,string>={home:"Overview",send:"Send something",devices:"Your devices",clipboard:"Clipboard",activity:"Activity",settings:"Settings",help:"Help & support"};
 return <div className={cx("app-shell",dark&&"dark")} data-theme={dark?"dark":"light"}>
  <div className="ambient ambient-one"/><div className="ambient ambient-two"/>
  <aside className={cx("sidebar",!sidebarOpen&&"sidebar-collapsed")}>
   <div className="brand-row"><div className="brand-mark"><ArrowLeftRight size={19} strokeWidth={2.3}/></div><div className="brand-copy"><span>bridge</span><small>YOUR DEVICES, TOGETHER</small></div><button className="icon-button sidebar-toggle" aria-label="Collapse sidebar" onClick={()=>setSidebarOpen(v=>!v)}><PanelLeftClose size={17}/></button></div>
   <div className="workspace-label">WORKSPACE</div>
   <nav className="nav-list" aria-label="Main navigation">{navPrimary.map(item=><NavButton key={item.id} item={item} active={page===item.id} collapsed={!sidebarOpen} onClick={()=>navigate(item.id)}/>)}</nav>
   <div className="sidebar-divider"/><div className="workspace-label">PREFERENCES</div>
   <nav className="nav-list"><NavButton item={{id:"settings",label:"Settings",icon:Settings2}} active={page==="settings"} collapsed={!sidebarOpen} onClick={()=>navigate("settings")}/><NavButton item={{id:"help",label:"Help & support",icon:CircleHelp}} active={page==="help"} collapsed={!sidebarOpen} onClick={()=>navigate("help")}/></nav>
   <div className="sidebar-spacer"/>
   <div className={cx("sidebar-device-card",!sidebarOpen&&"sidebar-device-mini")}><div className="device-card-icon"><ShieldCheck size={16}/></div><div className="device-card-copy"><strong>Private by design</strong><span>Your files stay yours.</span></div><span className="status-dot"/></div>
   <div className="sidebar-footer"><span>BRIDGE 0.1.0 · PREVIEW</span><span className="footer-online"><i/> Local demo</span></div>
  </aside>
  <main className="main-area">
   <header className="topbar"><div className="topbar-left"><button className="mobile-menu icon-button" onClick={()=>setSidebarOpen(v=>!v)} aria-label="Toggle navigation"><Menu size={19}/></button><span className="breadcrumb-root">Bridge</span><ChevronRight size={14} className="crumb-chevron"/><span className="breadcrumb-current">{pageTitle[page]}</span></div><div className="topbar-actions"><div className="connection-pill"><span className="status-dot"/><span>2 devices</span></div><button className="icon-button" aria-label="Open activity" onClick={()=>navigate("activity")}><Activity size={17}/></button><button className="avatar-button" aria-label="Local profile">A</button></div></header>
   <div className="content-scroll"><AnimatePresence mode="wait" initial={false}><motion.div key={page} className="page-content" initial={reducedMotion?false:{opacity:0,y:9}} animate={{opacity:1,y:0}} exit={reducedMotion?undefined:{opacity:0,y:-5}} transition={transition}>
    {page==="home"&&<HomePage transfers={transfers} onNavigate={navigate} onChoose={()=>fileInput.current?.click()} onDrop={onDrop} dragging={dragging} setDragging={setDragging}/>}
    {page==="send"&&<SendPage transfers={transfers} sendText={sendText} setSendText={setSendText} onChoose={()=>fileInput.current?.click()} onDrop={onDrop} dragging={dragging} setDragging={setDragging} onCopy={()=>{if(!sendText.trim())return;navigator.clipboard?.writeText(sendText);toastMessage("Text copied to clipboard");}}/>}
    {page==="devices"&&<DevicesPage peerStatus={peerStatus} peerBusy={peerBusy} peerError={peerError} onStartPeer={startPeerTransport} onStopPeer={stopPeerTransport} peerTargetId={peerTargetId} setPeerTargetId={setPeerTargetId} peerProbe={peerProbe} peerProbeBusy={peerProbeBusy} peerProbeError={peerProbeError} onProbePeer={probePeerEndpoint} onCopyPeerId={copyLocalPeerId} pairingInvite={pairingInvite} pairingInviteBusy={pairingInviteBusy} onCreatePairingInvite={createPairingInvite} onCopyPairingCode={copyPairingCode} pairingDeviceId={pairingDeviceId} setPairingDeviceId={setPairingDeviceId} pairingCode={pairingCode} setPairingCode={setPairingCode} pairingBusy={pairingBusy} pairingError={pairingError} onPairWithDevice={pairWithDevice} trustedDevices={trustedDevices} trustedBusy={trustedBusy} onRefreshTrustedDevices={refreshTrustedDevices} onRemoveTrustedDevice={removeTrustedDevice}/>}
    {page==="clipboard"&&<ClipboardPage items={clipboardItems} enabled={clipboardSync} onToggle={()=>setClipboardSync(v=>!v)} onClear={()=>setClipboardItems([])} onCopy={value=>{navigator.clipboard?.writeText(value);toastMessage("Copied");}}/>}
    {page==="activity"&&<ActivityPage transfers={transfers}/>}
    {page==="settings"&&<SettingsPage theme={theme} setTheme={value=>setTheme(value)} reducedMotion={reducedMotion} setReducedMotion={setReducedMotion} clipboardSync={clipboardSync} setClipboardSync={setClipboardSync} onSave={()=>{setSettingsSaved(true);toastMessage("Preferences saved on this device");}} settingsSaved={settingsSaved}/>}
    {page==="help"&&<HelpPage message={message} setMessage={value=>{setMessage(clampSupportMessage(value));setSupportStatus("");}} attachments={attachments} onAttach={()=>supportInput.current?.click()} onRemoveAttachment={removeAttachment} onEmail={openSupportEmail} supportStatus={supportStatus} faqOpen={faqOpen} setFaqOpen={setFaqOpen}/>}
   </motion.div></AnimatePresence></div>
   <footer className="statusbar"><span><span className="status-dot"/> All systems local in this preview</span><span>Made for your devices <span className="status-heart">·</span> Help is always close</span></footer>
  </main>
  <input ref={fileInput} className="visually-hidden" type="file" multiple onChange={event=>event.target.files&&addFiles(event.target.files)}/>
  <input ref={supportInput} className="visually-hidden" type="file" accept="image/*" multiple onChange={handleScreenshots}/>
  <AnimatePresence>{toast&&<motion.div className="toast" initial={{opacity:0,y:12,scale:.98}} animate={{opacity:1,y:0,scale:1}} exit={{opacity:0,y:8}} transition={transition}><Check size={16}/>{toast}</motion.div>}</AnimatePresence>
 </div>;
}
function NavButton({item,active,collapsed,onClick}:{item:{id:Page;label:string;icon:typeof Monitor};active:boolean;collapsed:boolean;onClick:()=>void}){
 const Icon=item.icon;return <button className={cx("nav-item",active&&"nav-item-active")} onClick={onClick} title={collapsed?item.label:undefined}><Icon size={17} strokeWidth={1.8}/><span>{item.label}</span>{active&&<motion.i layoutId="nav-active" className="nav-active-indicator"/>}</button>;
}
function PageHeading({eyebrow,title,subtitle,action}:{eyebrow?:string;title:string;subtitle?:string;action?:ReactNode}){
 return <div className="page-heading"><div>{eyebrow&&<div className="eyebrow">{eyebrow}</div>}<h1>{title}</h1>{subtitle&&<p>{subtitle}</p>}</div>{action&&<div className="heading-action">{action}</div>}</div>;
}
function SectionTitle({title,action}:{title:string;action?:ReactNode}){return <div className="section-title"><h2>{title}</h2>{action}</div>;}

function HomePage({transfers,onNavigate,onChoose,onDrop,dragging,setDragging}:{transfers:Transfer[];onNavigate:(page:Page)=>void;onChoose:()=>void;onDrop:(e:DragEvent<HTMLDivElement>)=>void;dragging:boolean;setDragging:(v:boolean)=>void}){
 return <div className="page-stack">
  <PageHeading eyebrow="YOUR PERSONAL CONNECTION" title="Your devices, together." subtitle="Move a file, share a thought, and keep your workflow flowing." action={<button className="button button-secondary" onClick={()=>onNavigate("devices")}><Plus size={16}/> Add a device</button>}/>
  <div className="device-overview-grid">
   <div className="device-overview-card device-current"><div className="device-card-top"><div className="device-icon-wrap"><Monitor size={21}/></div><span className="device-badge"><i/> This device</span></div><div><h3>Windows PC</h3><p>Ready to send and receive</p></div><div className="device-meta"><span><Wifi size={13}/> Connected locally</span><span>Bridge desktop</span></div></div>
   <div className="device-overview-card"><div className="device-card-top"><div className="device-icon-wrap phone"><Smartphone size={21}/></div><span className="device-badge"><i/> Demo device</span></div><div><h3>Android phone</h3><p>Example trusted device</p></div><div className="device-meta"><span><ShieldCheck size={13}/> Pairing not active</span><button className="text-button" onClick={()=>onNavigate("devices")}>Manage <ChevronRight size={13}/></button></div></div>
  </div>
  <div className={cx("dropzone",dragging&&"dropzone-active")} onDragOver={e=>{e.preventDefault();setDragging(true);}} onDragLeave={()=>setDragging(false)} onDrop={onDrop}><div className="dropzone-icon"><Upload size={22}/></div><div className="dropzone-copy"><h3>Send something over</h3><p>Drop files here, or choose them from your device.</p></div><button className="button button-primary" onClick={onChoose}><FolderOpen size={16}/> Choose files</button><span className="dropzone-footnote">Files are added to a local demo queue · No network transfer yet</span></div>
  <div className="home-lower-grid">
   <section className="surface-card recent-card"><SectionTitle title="Recent activity" action={<button className="text-button" onClick={()=>onNavigate("activity")}>View all <ArrowUpRight size={14}/></button>}/><div className="activity-list">{transfers.slice(0,3).map(item=><ActivityRow key={item.id} item={item}/>)}</div></section>
   <section className="surface-card quick-card"><SectionTitle title="Quick actions"/><button className="quick-action" onClick={()=>onNavigate("send")}><div className="quick-action-icon"><Send size={16}/></div><span><strong>Send text</strong><small>Share a note or a link</small></span><ChevronRight size={16}/></button><button className="quick-action" onClick={()=>onNavigate("clipboard")}><div className="quick-action-icon violet"><Clipboard size={16}/></div><span><strong>Open clipboard</strong><small>See your recent copies</small></span><ChevronRight size={16}/></button><button className="quick-action" onClick={()=>onNavigate("help")}><div className="quick-action-icon sand"><CircleHelp size={16}/></div><span><strong>Get help</strong><small>Guides and friendly support</small></span><ChevronRight size={16}/></button></section>
  </div>
  <div className="privacy-note"><ShieldCheck size={16}/><span><strong>Your space stays yours.</strong> Bridge is designed around direct device connections and private, opt-in sharing.</span><button onClick={()=>onNavigate("settings")}>Privacy settings <ArrowUpRight size={13}/></button></div>
 </div>;
}

function SendPage({transfers,sendText,setSendText,onChoose,onDrop,dragging,setDragging,onCopy}:{transfers:Transfer[];sendText:string;setSendText:(v:string)=>void;onChoose:()=>void;onDrop:(e:DragEvent<HTMLDivElement>)=>void;dragging:boolean;setDragging:(v:boolean)=>void;onCopy:()=>void}){
 return <div className="page-stack narrow-stack"><PageHeading eyebrow="SEND TO YOUR DEVICES" title="A little less friction." subtitle="Choose what you'd like to move, then pick where it should go."/>
  <section className="surface-card send-recipient"><div className="send-recipient-label"><span>Send to</span><span className="secure-label"><LockKeyhole size={12}/> Private pairing · UI preview</span></div><div className="recipient-select"><div className="device-icon-wrap phone"><Smartphone size={19}/></div><div><strong>Android phone</strong><small>Example device · Not paired yet</small></div><ChevronDown size={16}/></div></section>
  <div className={cx("dropzone send-dropzone",dragging&&"dropzone-active")} onDragOver={e=>{e.preventDefault();setDragging(true);}} onDragLeave={()=>setDragging(false)} onDrop={onDrop}><div className="dropzone-icon"><Files size={22}/></div><h3>Drop files to send</h3><p>Documents, photos, videos and more.</p><button className="button button-primary" onClick={onChoose}><FolderOpen size={16}/> Browse files</button><span className="dropzone-footnote">Files are queued locally in this preview. The transfer engine is a separate milestone.</span></div>
  <section className="surface-card send-text-card"><div className="send-text-head"><div><h3>Send a message</h3><p>Text, links, snippets — anything you can copy.</p></div><div className="mini-icon"><MessageCircle size={17}/></div></div><textarea value={sendText} onChange={e=>setSendText(e.target.value)} placeholder="Paste a link or write something short…" rows={4}/><div className="send-text-footer"><span>{sendText.length} characters</span><button className="button button-secondary" onClick={onCopy} disabled={!sendText.trim()}><Copy size={15}/> Copy text</button></div></section>
  <section className="surface-card queued-card"><SectionTitle title="Transfer queue" action={<span className="muted-count">{transfers.filter(item=>item.state==="Waiting").length} waiting</span>}/><div className="activity-list">{transfers.some(item=>item.state==="Waiting")?transfers.filter(item=>item.state==="Waiting").map(item=><ActivityRow key={item.id} item={item}/>):<div className="empty-inline"><CheckCheck size={18}/> Nothing waiting. You're all caught up.</div>}</div></section>
 </div>;
}

function DevicesPage({peerStatus,peerBusy,peerError,onStartPeer,onStopPeer,peerTargetId,setPeerTargetId,peerProbe,peerProbeBusy,peerProbeError,onProbePeer,onCopyPeerId,pairingInvite,pairingInviteBusy,onCreatePairingInvite,onCopyPairingCode,pairingDeviceId,setPairingDeviceId,pairingCode,setPairingCode,pairingBusy,pairingError,onPairWithDevice,trustedDevices,trustedBusy,onRefreshTrustedDevices,onRemoveTrustedDevice}:{peerStatus:PeerTransportStatus;peerBusy:boolean;peerError:string;onStartPeer:()=>void;onStopPeer:()=>void;peerTargetId:string;setPeerTargetId:(v:string)=>void;peerProbe:PeerProbe|null;peerProbeBusy:boolean;peerProbeError:string;onProbePeer:()=>void;onCopyPeerId:()=>void;pairingInvite:PairingInvite|null;pairingInviteBusy:boolean;onCreatePairingInvite:()=>void;onCopyPairingCode:()=>void;pairingDeviceId:string;setPairingDeviceId:(v:string)=>void;pairingCode:string;setPairingCode:(v:string)=>void;pairingBusy:boolean;pairingError:string;onPairWithDevice:()=>void;trustedDevices:TrustedDevice[];trustedBusy:boolean;onRefreshTrustedDevices:()=>void;onRemoveTrustedDevice:(id:string)=>void}){
 return <div className="page-stack"><PageHeading eyebrow="TRUSTED CONNECTIONS" title="Your devices." subtitle="Create a one-time invite, verify the other endpoint, and pair devices for this app session."/>
  <div className="device-list-card surface-card"><DeviceRow icon={<Monitor size={20}/>} name="This device" description="Current Bridge installation" current/><div className="device-security-foot"><ShieldCheck size={16}/><span>Pairing authenticates the peer endpoint over an encrypted iroh connection. Session approvals are cleared when the network engine stops.</span></div></div>
  <PeerTransportCard status={peerStatus} busy={peerBusy} error={peerError} onStart={onStartPeer} onStop={onStopPeer} onCopyPeerId={onCopyPeerId} peerTargetId={peerTargetId} setPeerTargetId={setPeerTargetId} peerProbe={peerProbe} peerProbeBusy={peerProbeBusy} peerProbeError={peerProbeError} onProbePeer={onProbePeer}/>
  <PairingPanel running={peerStatus.running} invite={pairingInvite} inviteBusy={pairingInviteBusy} onCreateInvite={onCreatePairingInvite} onCopyCode={onCopyPairingCode} deviceId={pairingDeviceId} setDeviceId={setPairingDeviceId} code={pairingCode} setCode={setPairingCode} busy={pairingBusy} error={pairingError} onPair={onPairWithDevice}/>
  <TrustedDevicesPanel devices={trustedDevices} busy={trustedBusy} onRefresh={onRefreshTrustedDevices} onRemove={onRemoveTrustedDevice}/>
 </div>;
}
function PairingPanel({running,invite,inviteBusy,onCreateInvite,onCopyCode,deviceId,setDeviceId,code,setCode,busy,error,onPair}:{running:boolean;invite:PairingInvite|null;inviteBusy:boolean;onCreateInvite:()=>void;onCopyCode:()=>void;deviceId:string;setDeviceId:(v:string)=>void;code:string;setCode:(v:string)=>void;busy:boolean;error:string;onPair:()=>void}){
 const inviteLink=invite?`bridge://pair?endpointId=${encodeURIComponent(invite.endpointId)}&code=${invite.code}`:"";
 const handleInviteInput=(value:string)=>{
  try{
   const url=new URL(value);
   if(url.protocol==="bridge:"&&url.hostname==="pair"){
    setDeviceId(url.searchParams.get("endpointId")??"");
    setCode((url.searchParams.get("code")??"").replace(/[^0-9]/g,"").slice(0,6));
    return;
   }
  }catch{/* A plain peer ID is also valid input. */}
  setDeviceId(value);
 };
 return <section className="surface-card pairing-panel">
  <div className="pairing-panel-head"><div><div className="eyebrow">ONE-TIME PAIRING</div><h2>Connect your own devices.</h2><p>Scan or share a short-lived invitation. Both devices must be open and online.</p></div><div className="pairing-panel-icon"><QrCode size={19}/></div></div>
  <div className="pairing-columns">
   <div className="pairing-invite-side"><h3>Invite this device</h3><p>Generate a three-minute invite. The QR contains this device ID and a one-use code.</p>
    {invite?<div className="pairing-code-card pairing-code-card-live"><div className="pairing-qr"><QRCodeSVG value={inviteLink} size={132} level="M" marginSize={4} title="Bridge one-time pairing invitation"/></div><span>ONE-TIME CODE · EXPIRES IN 3 MINUTES</span><strong>{invite.code.slice(0,3)} {invite.code.slice(3)}</strong><button className="button button-secondary" onClick={onCopyCode}><Copy size={14}/> Copy code</button></div>:<div className="pairing-code-empty"><LockKeyhole size={17}/><span>Your invite QR and code will appear here.</span></div>}
    <button className="button button-primary" onClick={onCreateInvite} disabled={!running||inviteBusy}><KeyRound size={15}/>{inviteBusy?"Creating invite…":invite?"Generate a new code":"Create pairing invite"}</button>
   </div>
   <div className="pairing-join-side"><h3>Join another device</h3><label htmlFor="pair-peer-id">Scan QR or paste an invite / device ID</label><input id="pair-peer-id" value={deviceId} onChange={e=>handleInviteInput(e.target.value)} placeholder="Paste Bridge invite link or endpoint ID" spellCheck={false}/><label htmlFor="pair-code">Six-digit invite code</label><input id="pair-code" value={code} onChange={e=>setCode(e.target.value.replace(/[^0-9]/g,"").slice(0,6))} placeholder="000000" inputMode="numeric" autoComplete="one-time-code" maxLength={6}/><button className="button button-primary" onClick={onPair} disabled={!running||busy||!deviceId.trim()||code.length!==6}>{busy?"Pairing…":"Pair devices"}</button><small>After scanning the QR with another camera, paste the copied Bridge invite link here. You can also enter the endpoint ID and code separately.</small></div>
  </div>
  {error&&<p className="peer-error">{error}</p>}
  <p className="pairing-session-note"><ShieldCheck size={13}/> Pairing is authenticated and one-use, but trusted devices are kept in memory only until the peer engine stops. Persistent identity storage is still required.</p>
 </section>;
}
function TrustedDevicesPanel({devices,busy,onRefresh,onRemove}:{devices:TrustedDevice[];busy:boolean;onRefresh:()=>void;onRemove:(id:string)=>void}){
 return <section className="surface-card trusted-devices-panel"><div className="section-title"><h2>Paired this session</h2><button className="text-button" onClick={onRefresh} disabled={busy}><RefreshCw size={13}/>{busy?"Refreshing…":"Refresh"}</button></div>
  {devices.length?devices.map(device=><div className="trusted-device-row" key={device.endpointId}><div className="device-icon-wrap phone"><Smartphone size={17}/></div><div><strong>{`Device ${device.endpointId.slice(0,10)}…`}</strong><span>{`Paired ${new Date(device.pairedAtUnix*1000).toLocaleString()}`}</span><code>{device.endpointId}</code></div><button className="icon-button" onClick={()=>onRemove(device.endpointId)} aria-label="Remove paired device"><Trash2 size={15}/></button></div>):<div className="empty-inline"><ShieldCheck size={17}/> No devices paired in this session yet.</div>}
 </section>;
}
function PeerTransportCard({status,busy,error,onStart,onStop,onCopyPeerId,peerTargetId,setPeerTargetId,peerProbe,peerProbeBusy,peerProbeError,onProbePeer}:{status:PeerTransportStatus;busy:boolean;error:string;onStart:()=>void;onStop:()=>void;onCopyPeerId:()=>void;peerTargetId:string;setPeerTargetId:(v:string)=>void;peerProbe:PeerProbe|null;peerProbeBusy:boolean;peerProbeError:string;onProbePeer:()=>void}){
 return <section className="surface-card peer-transport-card">
  <div className="peer-transport-head"><div className="peer-transport-icon"><Wifi size={18}/></div><div className="peer-transport-copy"><strong>Peer network engine</strong><span>{status.running?"Accepting encrypted Bridge control connections":"Not started on this device"}</span></div><span className={cx("transport-state",status.running&&"transport-state-on")}><i/>{status.running?"Running":"Stopped"}</span></div>
  {status.endpointId&&<div className="peer-id-row"><span>YOUR DEVICE ID · TEMPORARY UNTIL SECURE KEY STORAGE IS ADDED</span><div><code>{status.endpointId}</code><button className="icon-button" onClick={onCopyPeerId} aria-label="Copy this device ID"><Copy size={15}/></button></div></div>}
  <p className="peer-transport-description">The native iroh/QUIC endpoint accepts a versioned Bridge control handshake. The encrypted connection authenticates the endpoint key. This network engine is required for pairing and will later carry file streams.</p>
  {error&&<p className="peer-error">{error}</p>}
  <div className="peer-transport-actions">{status.running?<button className="button button-secondary" onClick={onStop} disabled={busy}><X size={14}/>{busy?"Stopping…":"Stop peer engine"}</button>:<button className="button button-primary" onClick={onStart} disabled={busy}><Wifi size={14}/>{busy?"Starting…":"Start peer engine"}</button>}<span><LockKeyhole size={12}/> Encrypted peer identity</span></div>
  <div className="peer-connect-area"><label htmlFor="peer-endpoint-id">Test encrypted reachability</label><div className="peer-connect-row"><input id="peer-endpoint-id" value={peerTargetId} onChange={e=>setPeerTargetId(e.target.value)} placeholder="Paste another Bridge endpoint ID" spellCheck={false}/><button className="button button-secondary" onClick={onProbePeer} disabled={!status.running||peerProbeBusy||!peerTargetId.trim()}>{peerProbeBusy?"Connecting…":"Test"}</button></div><small>Both devices must have Bridge open and the peer engine running.</small></div>
  {peerProbeError&&<p className="peer-error">{peerProbeError}</p>}
  {peerProbe&&<div className="peer-probe-result"><CheckCheck size={16}/><div><strong>Encrypted connection verified</strong><span>{peerProbe.remoteEndpointId} · Bridge protocol v{peerProbe.protocolVersion}</span></div></div>}
 </section>;
}
function DeviceRow({icon,name,description,current=false}:{icon:ReactNode;name:string;description:string;current?:boolean}){
 return <div className="device-row"><div className="device-icon-wrap">{icon}</div><div className="device-row-copy"><strong>{name}</strong><span>{description}</span></div>{current?<span className="current-device-label">This device</span>:<span className="device-badge"><i/> Trusted</span>}</div>;
}

function ClipboardPage({items,enabled,onToggle,onClear,onCopy}:{items:typeof initialClipboard;enabled:boolean;onToggle:()=>void;onClear:()=>void;onCopy:(v:string)=>void}){
 return <div className="page-stack"><PageHeading eyebrow="SHARE THE SMALL THINGS" title="Clipboard." subtitle="A quiet place for links and snippets you choose to move between devices." action={<button className="button button-secondary" onClick={onClear} disabled={!items.length}><Trash2 size={15}/> Clear history</button>}/>
  <section className="surface-card clipboard-preference"><div className="preference-icon"><Clipboard size={18}/></div><div className="preference-copy"><strong>Clipboard sync</strong><span>{enabled?"Opted in on this device":"Off until you choose to turn it on"}</span></div><Toggle value={enabled} onChange={onToggle}/></section>
  <div className="clipboard-list-heading"><h2>Recent items</h2><span>{items.length} items · demo data</span></div>
  {items.length?<div className="clipboard-items">{items.map(item=><article className="surface-card clipboard-item" key={item.id}><div className="clipboard-type-icon">{item.type==="Link"?<Link2Icon/>:<FileText size={17}/>}</div><div className="clipboard-item-main"><span className="clipboard-type">{item.type} <i/> {item.time}</span><p>{item.value}</p><small>{item.note}</small></div><button className="icon-button" onClick={()=>onCopy(item.value)} aria-label="Copy item"><Copy size={16}/></button></article>)}</div>:<div className="surface-card empty-state"><Clipboard size={25}/><h3>Nothing here yet</h3><p>Clipboard entries will appear here once you choose to add them.</p></div>}
  <p className="subtle-help"><LockKeyhole size={13}/> The toggle is a preference mock in this preview. System clipboard monitoring is not active.</p>
 </div>;
}
function Link2Icon(){return <ArrowUpRight size={17}/>;}

function ActivityPage({transfers}:{transfers:Transfer[]}){
 const [filter,setFilter]=useState("All");
 const visible=transfers.filter(item=>filter==="All"||(filter==="Sent"&&item.direction==="sent")||(filter==="Received"&&item.direction==="received"));
 return <div className="page-stack"><PageHeading eyebrow="YOUR RECENT MOVES" title="Activity." subtitle="A simple record of items added to your local demo queue."/><div className="filter-tabs">{["All","Sent","Received"].map(value=><button key={value} className={cx(filter===value&&"filter-active")} onClick={()=>setFilter(value)}>{value}</button>)}</div><section className="surface-card activity-full">{visible.map(item=><ActivityRow key={item.id} item={item}/>)}{!visible.length&&<div className="empty-inline">No activity in this view.</div>}</section><p className="subtle-help"><Clock3 size={13}/> These are local demo entries, not completed network transfers.</p></div>;
}
function ActivityRow({item}:{item:Transfer}){
 return <div className="activity-row"><div className="file-icon"><File size={17}/></div><div className="activity-file"><strong>{item.name}</strong><span>{item.size} <i/> {item.time}</span></div><span className={cx("activity-state",item.state==="Complete"&&"state-complete",item.state==="Failed"&&"state-failed")}>{item.state==="Complete"?<Check size={12}/>:<Clock3 size={12}/>} {item.state}</span></div>;
}

function SettingsPage({theme,setTheme,reducedMotion,setReducedMotion,clipboardSync,setClipboardSync,onSave,settingsSaved}:{theme:Theme;setTheme:(v:Theme)=>void;reducedMotion:boolean;setReducedMotion:(v:boolean)=>void;clipboardSync:boolean;setClipboardSync:(v:boolean)=>void;onSave:()=>void;settingsSaved:boolean}){
 return <div className="page-stack narrow-stack"><PageHeading eyebrow="MAKE BRIDGE YOURS" title="Settings." subtitle="A few thoughtful choices to make Bridge feel right on every device."/>
  <SettingsGroup title="Appearance" description="Choose the look that feels comfortable."><div className="theme-options">{(["light","dark","system"] as Theme[]).map(value=><button key={value} className={cx("theme-option",theme===value&&"theme-option-active")} onClick={()=>setTheme(value)}><span className={`theme-swatch theme-${value}`}>{value==="light"?<Sun size={17}/>:value==="dark"?<Moon size={17}/>:<Monitor size={17}/>}</span><strong>{value[0].toUpperCase()+value.slice(1)}</strong>{theme===value&&<Check size={14}/>}</button>)}</div></SettingsGroup>
  <SettingsGroup title="Motion" description="Keep transitions gentle, or reduce movement across the app."><SettingLine title="Reduced motion" subtitle="Shorten or remove interface movement"><Toggle value={reducedMotion} onChange={setReducedMotion}/></SettingLine></SettingsGroup>
  <SettingsGroup title="Privacy" description="Sharing features stay off until you enable them."><SettingLine title="Clipboard sharing" subtitle="Allow selected clipboard items to move between trusted devices"><Toggle value={clipboardSync} onChange={setClipboardSync}/></SettingLine><div className="settings-note"><LockKeyhole size={15}/> No background clipboard monitoring is enabled in this preview.</div></SettingsGroup>
  <SettingsGroup title="About Bridge" description="A free utility built around your devices."><SettingLine title="Version" subtitle="0.1.0 · Early preview" right={<span className="setting-value">Preview</span>}/><SettingLine title="Support" subtitle="Telegram @vvpse · linka.comp@gmail.com" right={<span className="setting-value">Available</span>}/></SettingsGroup>
  <div className="save-settings-row"><span>{settingsSaved?"Your preferences are saved on this device.":"Changes are saved locally as you make them."}</span><button className="button button-primary" onClick={onSave}><Check size={15}/> Save preferences</button></div>
 </div>;
}
function SettingsGroup({title,description,children}:{title:string;description:string;children:ReactNode}){return <section className="surface-card settings-group"><div className="settings-group-heading"><h3>{title}</h3><p>{description}</p></div>{children}</section>;}
function SettingLine({title,subtitle,right,children}:{title:string;subtitle:string;right?:ReactNode;children?:ReactNode}){return <div className="setting-line"><div><strong>{title}</strong><span>{subtitle}</span></div>{right??children}</div>;}
function Toggle({value,onChange}:{value:boolean;onChange:(v:boolean)=>void}){return <button className={cx("toggle",value&&"toggle-on")} aria-pressed={value} onClick={()=>onChange(!value)}><i/></button>;}

const faqs=[
 {q:"How do I connect my phone and computer?",a:"Open Devices on both devices, start Pair a device on one of them, then confirm the same short-lived code on the other. Only approve pairing requests you started yourself."},
 {q:"Do my files go through a cloud account?",a:"Bridge is being built around direct device-to-device transfers. A relay may help two devices find a network path when a direct connection is unavailable, but it is not intended as file storage. The transport is not active in this preview yet."},
 {q:"Can I turn off clipboard sharing?",a:"Yes. Clipboard sharing will be opt-in and can be turned off in Clipboard or Settings. The preview does not monitor your system clipboard in the background."},
 {q:"What should I include in a support request?",a:"Describe what you expected, what happened instead, and the steps that reproduce it. Add a screenshot when useful. Remove passwords, private documents and sensitive details first."},
 {q:"Why is a transfer waiting?",a:"This preview only demonstrates the queue interface. In the full app, waiting states will explain whether a device is offline, a transfer needs approval, or the network path is being established."}
];
function HelpPage({message,setMessage,attachments,onAttach,onRemoveAttachment,onEmail,supportStatus,faqOpen,setFaqOpen}:{message:string;setMessage:(v:string)=>void;attachments:Attachment[];onAttach:()=>void;onRemoveAttachment:(id:string)=>void;onEmail:()=>void;supportStatus:string;faqOpen:number|null;setFaqOpen:(v:number|null)=>void}){
 return <div className="page-stack"><PageHeading eyebrow="WE'RE HERE TO HELP" title="How can we help?" subtitle="Find a quick answer, or tell us what isn't working. We read every message."/>
  <div className="help-grid"><section className="surface-card faq-card"><SectionTitle title="Frequently asked questions"/><div className="faq-list">{faqs.map((faq,index)=><div className={cx("faq-item",faqOpen===index&&"faq-item-open")} key={faq.q}><button onClick={()=>setFaqOpen(faqOpen===index?null:index)}><span>{faq.q}</span><ChevronDown size={16}/></button><AnimatePresence initial={false}>{faqOpen===index&&<motion.div className="faq-answer" initial={{height:0,opacity:0}} animate={{height:"auto",opacity:1}} exit={{height:0,opacity:0}} transition={{duration:.24,ease:"easeOut"}}><p>{faq.a}</p></motion.div>}</AnimatePresence></div>)}</div></section>
   <section className="surface-card contact-card"><div className="contact-heading-icon"><MessageCircle size={19}/></div><h3>Talk to a real person.</h3><p>Send a note and a screenshot. A short, clear description helps us get to the point faster.</p>
    <label className="field-label" htmlFor="support-message">Your message <span>Required</span></label><textarea id="support-message" className="support-textarea" value={message} onChange={e=>setMessage(e.target.value)} placeholder="Tell us what happened, what you expected, and what device you're using…" rows={5}/><div className="character-count"><span>{Array.from(message).length} / {SUPPORT_MESSAGE_LIMIT}</span><span>Keep it under 800 characters</span></div>
    <button className="attachment-button" onClick={onAttach}><FileImage size={15}/> Add screenshots <span>Up to 5 images · 8 MB each</span></button>
    {attachments.length>0&&<div className="attachment-list">{attachments.map(item=><div className="attachment-preview" key={item.id}><img src={item.url} alt={item.file.name}/><div><strong>{item.file.name}</strong><span>{formatBytes(item.file.size)}</span></div><button className="icon-button" onClick={()=>onRemoveAttachment(item.id)} aria-label="Remove attachment"><X size={15}/></button></div>)}</div>}
    <button className="button button-primary support-submit" onClick={onEmail} disabled={!message.trim()}><Mail size={15}/> Open support email <ArrowUpRight size={14}/></button>{supportStatus&&<p className="support-status">{supportStatus}</p>}
    <div className="contact-divider"><span>OR REACH US DIRECTLY</span></div><a className="contact-link" href="https://t.me/vvpse" target="_blank" rel="noreferrer"><div className="contact-link-icon telegram"><Send size={16}/></div><span><strong>Telegram</strong><small>@vvpse</small></span><ArrowUpRight size={15}/></a><a className="contact-link" href="mailto:linka.comp@gmail.com"><div className="contact-link-icon email"><Mail size={16}/></div><span><strong>Email</strong><small>linka.comp@gmail.com</small></span><ArrowUpRight size={15}/></a>
    <p className="attachment-disclaimer">When opening email, attach selected screenshots manually. Email links cannot attach local files.</p>
   </section>
  </div>
 </div>;
}
