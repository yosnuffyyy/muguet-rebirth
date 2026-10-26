// Original library composition; no proprietary launcher assets.
import SwiftUI
import UIKit
import UniformTypeIdentifiers

@_silgen_name("rebirth_prepare") private func prepareLaunch(_ root:UnsafePointer<CChar>,_ json:UnsafePointer<CChar>,_ saves:UnsafePointer<CChar>)->Bool
@_silgen_name("rebirth_error") private func launchError()->UnsafePointer<CChar>?
@_silgen_name("rebirth_logs_start") private func startLogs(_ root:UnsafePointer<CChar>)->Bool
@_silgen_name("rebirth_logs_identify") private func identifyLogs(_ identity:UnsafePointer<CChar>)
@_silgen_name("rebirth_logs_export") private func exportLogs(_ root:UnsafePointer<CChar>)->UnsafePointer<CChar>?
@_silgen_name("muguet_has_game_code") private func hasGameCode()->Bool
@_silgen_name("os_proc_available_memory") private func osProcAvailableMemory()->Int

final class SelectedFolder: ObservableObject {
    static let shared=SelectedFolder()
    @Published var url:URL?
    @Published var error:String?
    private var scoped=false
    init() {
        guard let data=UserDefaults.standard.data(forKey:"gameFolderBookmark") else{return}
        do {
            var stale=false
            let restored=try URL(resolvingBookmarkData:data,options:[],relativeTo:nil,bookmarkDataIsStale:&stale)
            try select(restored,save:stale)
        } catch {self.error="Folder access needs renewal: \(error.localizedDescription)"}
    }
    func select(_ next:URL,save:Bool=true)throws {
        let access=next.startAccessingSecurityScopedResource()
        do {
            guard try next.resourceValues(forKeys:[.isDirectoryKey]).isDirectory == true else{throw CocoaError(.fileReadUnsupportedScheme)}
            if save {let bookmark=try next.bookmarkData(options:[],includingResourceValuesForKeys:nil,relativeTo:nil);UserDefaults.standard.set(bookmark,forKey:"gameFolderBookmark")}
        } catch {if access{next.stopAccessingSecurityScopedResource()};throw error}
        if scoped{url?.stopAccessingSecurityScopedResource()}
        url=next;scoped=access;error=nil
    }
    // Held for the entire engine session. Upstream exits the process on stop;
    // never release an active archive scope when the launcher is dismissed.
    deinit {if scoped{url?.stopAccessingSecurityScopedResource()}}
}
enum LibraryPage:String,CaseIterable {case home="Home",game="GTA V",graphics="Graphics",gameplay="Gameplay",controls="Controls",audio="Audio",advanced="Advanced",files="Game Files",diagnostics="Diagnostics"
    var icon:String{switch self{case .home:return "square.grid.2x2";case .game:return "gamecontroller";case .graphics:return "display";case .gameplay:return "person.fill";case .controls:return "dpad";case .audio:return "speaker.wave.2";case .advanced:return "slider.horizontal.3";case .files:return "folder";case .diagnostics:return "waveform.path.ecg"}}
}

// MARK: - Visual language
// Deep blue-black ground, and one accent that follows the selected mode:
// sunset orange for Story, ocean teal for Sandbox. Flat surfaces, hairline rules,
// italic poster-weight headings. Motion is one-shot (no repeat-forever loops),
// stays off the settings pages, and honours Reduce Motion.
private enum Theme {
    static let ink=Color(red:0.039,green:0.055,blue:0.075)
    static let surface=Color(red:0.075,green:0.097,blue:0.125)
    static let line=Color.white.opacity(0.1)
    static let muted=Color.white.opacity(0.64)
    static let good=Color(red:0.35,green:0.86,blue:0.55)
    static let warn=Color(red:1.0,green:0.78,blue:0.25)
    static let bad=Color(red:1.0,green:0.38,blue:0.34)
    static func accent(_ mode:Int)->Color {mode==1 ? Color(red:1.0,green:0.48,blue:0.18) : Color(red:0.18,green:0.83,blue:0.75)}
}

private struct PanelStyle:ViewModifier {
    func body(content:Content)->some View {
        content.background(Theme.surface.opacity(0.9),in:RoundedRectangle(cornerRadius:6))
            .overlay(RoundedRectangle(cornerRadius:6).stroke(Theme.line,lineWidth:1))
    }
}
private extension View {
    func panel()->some View {modifier(PanelStyle())}
    func reveal(_ delay:Double)->some View {modifier(RevealStyle(delay:delay))}
}

private struct RevealStyle:ViewModifier {
    let delay:Double
    @Environment(\.accessibilityReduceMotion) private var reduceMotion
    @State private var shown=false
    func body(content:Content)->some View {
        content.opacity(shown || reduceMotion ? 1:0).offset(y:shown || reduceMotion ? 0:14)
            .onAppear{withAnimation(.easeOut(duration:0.45).delay(delay)){shown=true}}
    }
}

private struct PressStyle:ButtonStyle {
    func makeBody(configuration:Configuration)->some View {
        configuration.label.scaleEffect(configuration.isPressed ? 0.96:1).opacity(configuration.isPressed ? 0.85:1)
            .animation(.easeOut(duration:0.12),value:configuration.isPressed)
    }
}

private struct SectionHeader:View {
    let index:String
    let title:String
    var body:some View {
        HStack(spacing:10) {
            Text(index).font(.caption2.monospacedDigit().weight(.bold)).foregroundStyle(Color.accentColor)
            Text(title.uppercased()).font(.caption.weight(.bold)).kerning(1.8).foregroundStyle(Theme.muted)
            Rectangle().fill(Theme.line).frame(height:1)
        }
        .accessibilityElement(children:.combine).accessibilityAddTraits(.isHeader)
    }
}

// MARK: - Mode artwork
// Artwork is looked up by name in the app's asset catalog (see tools/prepare-icons.mjs).
// Builds without the catalog, such as the UI-test harness, draw original fallback art.
// The picture is placed explicitly so Story keeps its faces at the top of the frame.
private struct FallbackArt:View {
    let mode:Int
    var body:some View {
        ZStack(alignment:.bottom) {
            LinearGradient(colors:mode==1 ? [Color(red:0.42,green:0.22,blue:0.12),Color(red:0.08,green:0.07,blue:0.07)] : [Color(red:0.16,green:0.42,blue:0.50),Color(red:0.06,green:0.12,blue:0.15)],startPoint:.top,endPoint:.bottom)
            HStack(alignment:.bottom,spacing:4) {ForEach(0..<24,id:\.self){i in Rectangle().fill(Color.black.opacity(0.28)).frame(height:CGFloat(30+(i*37)%90))}}.padding(.horizontal,12)
        }
    }
}
private struct ModeArtwork:View {
    let mode:Int
    var body:some View {
        GeometryReader { g in
            if let image=UIImage(named:mode==1 ? "ModeStory":"ModeSandbox") {
                let natural=image.size
                let scale=max(g.size.width/max(natural.width,1),g.size.height/max(natural.height,1))
                let w=natural.width*scale
                let h=natural.height*scale
                let focusY:CGFloat = mode==1 ? 0:0.35
                Image(uiImage:image).resizable().frame(width:w,height:h)
                    .position(x:w/2-(w-g.size.width)/2,y:h/2-(h-g.size.height)*focusY)
            } else {
                FallbackArt(mode:mode)
            }
        }
        .clipped()
    }
}
// Slow one-shot push-in on the Home stage only. Re-created per mode (via .id).
private struct DriftingArtwork:View {
    let mode:Int
    let drift:Bool
    @Environment(\.accessibilityReduceMotion) private var reduceMotion
    @State private var drifted=false
    var body:some View {
        let target=drifted && drift && !reduceMotion
        ModeArtwork(mode:mode)
            .scaleEffect(target ? 1.08:1,anchor:mode==1 ? .topLeading:.bottomTrailing)
            .animation(target ? .linear(duration:30):.easeOut(duration:0.4),value:target)
            .onAppear{drifted=true}
    }
}
// Full-bleed picture behind everything. Bright on Home; a dark wash behind settings pages.
private struct Backdrop:View {
    let mode:Int
    let focused:Bool
    var body:some View {
        ZStack {
            Theme.ink
            DriftingArtwork(mode:mode,drift:focused).id(mode).transition(.opacity)
            LinearGradient(colors:[Theme.ink.opacity(0.94),Theme.ink.opacity(0.55),Theme.ink.opacity(0.08)],startPoint:.leading,endPoint:.trailing).opacity(focused ? 1:0)
            LinearGradient(colors:[.clear,Theme.ink.opacity(0.9)],startPoint:.center,endPoint:.bottom).opacity(focused ? 1:0)
            LinearGradient(colors:[Theme.ink.opacity(0.85),.clear],startPoint:.top,endPoint:UnitPoint(x:0.5,y:0.3))
            Theme.ink.opacity(focused ? 0:0.9)
        }
        .animation(.easeInOut(duration:0.6),value:mode)
        .animation(.easeInOut(duration:0.4),value:focused)
        .ignoresSafeArea()
        .accessibilityHidden(true)
    }
}
private struct ModeTile:View {
    let mode:Int
    let selected:Bool
    let action:()->Void
    var body:some View {
        Button(action:action) {
            ZStack(alignment:.bottomLeading) {
                ModeArtwork(mode:mode).saturation(selected ? 1:0.15)
                LinearGradient(colors:[.clear,Color.black.opacity(0.78)],startPoint:.center,endPoint:.bottom)
                HStack {
                    Text(mode==1 ? "STORY":"SANDBOX").font(.caption.weight(.heavy)).kerning(1.8).foregroundStyle(.white)
                    Spacer(minLength:4)
                    if selected {Image(systemName:"checkmark.circle.fill").foregroundStyle(Color.accentColor).accessibilityHidden(true)}
                }.padding(8)
            }
            .aspectRatio(16/9,contentMode:.fit)
            .clipShape(RoundedRectangle(cornerRadius:4))
            .overlay(RoundedRectangle(cornerRadius:4).stroke(selected ? Color.accentColor:Theme.line,lineWidth:selected ? 2:1))
            .opacity(selected ? 1:0.72)
            .scaleEffect(selected ? 1:0.97)
            .animation(.spring(response:0.4,dampingFraction:0.8),value:selected)
        }
        .buttonStyle(PressStyle())
        .accessibilityLabel(mode==1 ? "Story mode":"Sandbox mode")
        .accessibilityAddTraits(selected ? .isSelected:[])
    }
}

// MARK: - Memory entitlement probe
// Two independent signals, never merged into one number:
//  1. whether the installed provisioning profile grants Increased Memory Limit
//  2. the measured ceiling: current footprint + what iOS says is still available.
private struct MemoryStatus {
    var physical:UInt64=ProcessInfo.processInfo.physicalMemory
    var ceiling:UInt64?
    var profile:Bool?
    static func measure()->MemoryStatus {
        var status=MemoryStatus()
        var info=task_vm_info_data_t()
        var count=mach_msg_type_number_t(MemoryLayout<task_vm_info_data_t>.size/MemoryLayout<integer_t>.size)
        let result=withUnsafeMutablePointer(to:&info){pointer in
            pointer.withMemoryRebound(to:integer_t.self,capacity:Int(count)){task_info(mach_task_self_,task_flavor_t(TASK_VM_INFO),$0,&count)}
        }
        let available=osProcAvailableMemory()
        if result==KERN_SUCCESS && available>0 {status.ceiling=info.phys_footprint+UInt64(available)}
        if let path=Bundle.main.path(forResource:"embedded",ofType:"mobileprovision"),let data=try? Data(contentsOf:URL(fileURLWithPath:path)) {
            status.profile=data.range(of:Data("com.apple.developer.kernel.increased-memory-limit".utf8)) != nil
        }
        return status
    }
    private static func gigabytes(_ bytes:UInt64)->String {String(format:"%.2f GB",Double(bytes)/1_073_741_824)}
    var ratio:Double? {ceiling.map{Double($0)/Double(max(physical,1))}}
    private var raised:Bool {(ratio ?? 0) >= 0.55}
    var tone:Color {
        if profile==true && raised {return Theme.good}
        if profile==true || (profile==nil && raised) {return Theme.warn}
        return Theme.bad
    }
    var title:String {
        if profile==true && raised {return "Extra RAM is working"}
        if profile==true {return "Signed, but the memory ceiling looks standard"}
        if profile==false {return "Extra RAM is not active"}
        return raised ? "Extra RAM looks active (profile unreadable)":"Extra RAM not detected"
    }
    var shortLabel:String {
        guard let ceiling else {return "Not measurable"}
        return (raised ? "Extended · ":"Standard · ")+MemoryStatus.gigabytes(ceiling)
    }
    var profileText:String {
        switch profile {case .some(true):return "Granted by the signing profile";case .some(false):return "Missing from the signing profile";case .none:return "No profile to read"}
    }
    var ceilingText:String {
        guard let ceiling,let ratio else {return "Not measurable here"}
        return "\(MemoryStatus.gigabytes(ceiling)) · \(Int((ratio*100).rounded()))% of device RAM"
    }
    var deviceText:String {MemoryStatus.gigabytes(physical)}
}

// MARK: - Settings rows
// Picker's menu presentation does not render its title in a plain VStack.
// Own the label and both columns here, independent of the control's style.
private struct LauncherSettingRow<Control:View>:View {
    let title:String
    let id:String
    @ViewBuilder let control:()->Control
    var body:some View {
        HStack(alignment:.center,spacing:16) {
            Text(title).font(.body).foregroundStyle(.primary)
                .fixedSize(horizontal:false,vertical:true)
                .frame(maxWidth:.infinity,alignment:.leading)
                .accessibilityIdentifier("graphics.\(id).label")
            control().frame(maxWidth:.infinity,alignment:.trailing)
        }
        .frame(minHeight:44).padding(.vertical,4).padding(.horizontal,14)
        .overlay(alignment:.bottom) {Rectangle().fill(Theme.line).frame(height:1).padding(.leading,14).accessibilityHidden(true)}
        .accessibilityElement(children:.contain)
    }
}

private struct LauncherSettingPicker<Value:Hashable>:View {
    let title:String
    let id:String
    @Binding var selection:Value
    let options:[(Value,String)]
    var savedTitle:String? = nil
    private var currentTitle:String {
        options.first(where:{$0.0==selection})?.1 ?? savedTitle ?? String(describing:selection)
    }
    var body:some View {
        LauncherSettingRow(title:title,id:id) {
            Menu {
                Picker(title,selection:$selection) {
                    // Preserve valid migrated/custom values absent from the quick choices.
                    if !options.contains(where:{$0.0==selection}) {Text(currentTitle).tag(selection)}
                    ForEach(options,id:\.0) {Text($0.1).tag($0.0)}
                }.pickerStyle(.inline)
            } label: {
                HStack(spacing:8) {
                    Text(currentTitle).multilineTextAlignment(.trailing).foregroundStyle(Color.accentColor)
                        .fixedSize(horizontal:false,vertical:true)
                    Image(systemName:"chevron.up.chevron.down").font(.caption).foregroundStyle(Theme.muted)
                        .accessibilityHidden(true)
                }
                .frame(maxWidth:.infinity,minHeight:44,alignment:.trailing)
                .padding(.horizontal,12).padding(.vertical,4)
                .background(Theme.ink.opacity(0.7),in:RoundedRectangle(cornerRadius:4))
                .overlay(RoundedRectangle(cornerRadius:4).stroke(Theme.line,lineWidth:1))
                .contentShape(Rectangle())
            }
            .accessibilityIdentifier("graphics.\(id).control")
            .accessibilityLabel(title).accessibilityValue(currentTitle)
            .onChange(of:selection){_ in UISelectionFeedbackGenerator().selectionChanged()}
        }
    }
}

struct LauncherView:View {
    private static let installed = RebirthBuildIdentity(info: Bundle.main.infoDictionary ?? [:])
    @Environment(\.dynamicTypeSize) private var typeSize
    @AppStorage("rebirth.displayProbe") private var displayProbe=false
    @State private var s=RebirthSettings()
    @State private var page=LibraryPage.home
    @State private var picker=false
    @State private var error:String?
    @State private var loaded=false
    @State private var starting=false
    @State private var exporting=false
    @State private var sharedLog:SharedLog?
    @State private var memory=MemoryStatus.measure()
    @Namespace private var tabMark
    @ObservedObject private var folder=SelectedFolder.shared
    let play:()->Void
    init(initialPage:LibraryPage = .home,play:@escaping()->Void){_page=State(initialValue:initialPage);self.play=play}
    private var accent:Color {Theme.accent(s.mode)}
    private var canPlay:Bool {loaded && folder.url != nil && hasGameCode()}
    private var isStage:Bool {page==.home || page==.game}
    var body:some View {
        GeometryReader { geometry in
            ZStack {
                Backdrop(mode:s.mode,focused:isStage)
                VStack(spacing:0) {
                    topBar
                    ZStack {
                        ScrollView {VStack(alignment:.leading,spacing:18){
                            if !isStage {
                                HStack(alignment:.firstTextBaseline,spacing:12){
                                    Text(page.rawValue.uppercased()).font(.title2.weight(.black).italic()).kerning(1)
                                    Spacer(minLength:8)
                                    Text(Self.installed.summary).font(.caption.monospaced()).foregroundStyle(Theme.muted).fixedSize(horizontal:false,vertical:true).multilineTextAlignment(.trailing).accessibilityIdentifier("rebirth.installedVersion")
                                }
                            }
                            if let error {Text(error).foregroundStyle(Theme.bad).fixedSize(horizontal:false,vertical:true)}
                            content(geometry.size)
                        }.padding(.horizontal,22).padding(.vertical,isStage ? 8:14).frame(maxWidth:isStage ? .infinity:900,alignment:.leading)}
                        .accessibilityIdentifier("launcher.content")
                        .id(page)
                        .transition(.asymmetric(insertion:AnyTransition.opacity.combined(with:.offset(x:18,y:0)),removal:.opacity))
                    }.frame(maxWidth:.infinity,maxHeight:.infinity)
                    playBar
                }
            }
            .tint(accent).preferredColorScheme(.dark)
            .animation(.easeInOut(duration:0.5),value:s.mode)
        }
        .onAppear{do{let support=FileManager.default.urls(for:.applicationSupportDirectory,in:.userDomainMask)[0];let logRoot=support.appendingPathComponent("RebirthLogs");if !logRoot.path.withCString({startLogs($0)}){error=launchError().map{String(cString:$0)}}else{Self.installed.diagnostic.withCString{identifyLogs($0)}};s=try RebirthStore.load();loaded=true}catch{self.error="Saved settings could not be read. Reset settings to recover."}}
        .onChange(of:s){value in guard loaded else{return};do{try RebirthStore.save(value)}catch{self.error=error.localizedDescription}}
        .sheet(item:$sharedLog){log in LogShare(url:log.url)}
        .disabled(starting)
        .fileImporter(isPresented:$picker,allowedContentTypes:[.folder]){result in do{try folder.select(result.get());error=nil}catch{self.error=error.localizedDescription}}
    }
    private func select(_ item:LibraryPage) {
        guard item != page else{return}
        UISelectionFeedbackGenerator().selectionChanged()
        withAnimation(.easeInOut(duration:0.3)){page=item}
    }
    private func choose(mode:Int) {
        guard s.mode != mode else{return}
        UISelectionFeedbackGenerator().selectionChanged()
        withAnimation(.spring(response:0.45,dampingFraction:0.85)){s.mode=mode}
    }
    private var topBar:some View {
        HStack(spacing:14) {
            VStack(alignment:.leading,spacing:3) {
                Text("REBIRTH").font(.headline.weight(.black).italic()).kerning(3).fixedSize()
                Rectangle().fill(Color.accentColor).frame(width:26,height:3)
            }
            if typeSize.isAccessibilitySize {
                Menu {ForEach(LibraryPage.allCases,id:\.self){item in Button(item.rawValue){select(item)}}} label:{Label(page.rawValue,systemImage:page.icon).frame(minHeight:44)}
                    .accessibilityLabel("Launcher section")
                Spacer(minLength:0)
            } else {
                ScrollView(.horizontal,showsIndicators:false){HStack(spacing:2){navigation}}
            }
        }
        .padding(.horizontal,18).frame(minHeight:48)
    }
    @ViewBuilder private var navigation:some View {
        ForEach(LibraryPage.allCases,id:\.self){item in
            Button{select(item)} label:{
                HStack(spacing:6){
                    Image(systemName:item.icon).font(.footnote).accessibilityHidden(true)
                    Text(item.rawValue).font(.subheadline.weight(.semibold)).lineLimit(1)
                }
                .padding(.horizontal,12).frame(minHeight:44)
                .overlay(alignment:.bottom){
                    if page==item {Rectangle().fill(Color.accentColor).frame(height:3).matchedGeometryEffect(id:"tabMark",in:tabMark)}
                }
                .contentShape(Rectangle())
            }
            .buttonStyle(PressStyle()).foregroundStyle(page==item ? Color.white:Theme.muted)
            .accessibilityAddTraits(page==item ? .isSelected:[])
        }
    }
    private var playBar:some View {
        VStack(spacing:0) {
            Rectangle().fill(LinearGradient(colors:[Color.accentColor.opacity(0.7),Theme.line,.clear],startPoint:.leading,endPoint:.trailing)).frame(height:1)
            HStack(spacing:16) {
                VStack(alignment:.leading,spacing:2) {
                    Text(s.mode==1 ? "STORY MODE" : "SANDBOX MODE").font(.caption.weight(.bold)).kerning(1.4).foregroundStyle(Color.accentColor)
                    Text("\(s.preset) · \(s.resolution) · \(s.fps) FPS cap").font(.caption).foregroundStyle(Theme.muted).fixedSize(horizontal:false,vertical:true)
                }
                Spacer()
                Button{UIImpactFeedbackGenerator(style:.medium).impactOccurred();start()} label:{
                    HStack(spacing:10) {
                        Text(starting ? "PREPARING":"PLAY").font(.headline.weight(.black).italic()).kerning(2)
                        Image(systemName:starting ? "hourglass":"play.fill").accessibilityHidden(true)
                    }
                    .foregroundStyle(.black).padding(.horizontal,30).frame(minHeight:46).background(Color.accentColor,in:RoundedRectangle(cornerRadius:4))
                }
                .buttonStyle(PressStyle()).disabled(!canPlay).opacity(canPlay ? 1:0.45)
                .animation(.easeInOut(duration:0.3),value:canPlay)
            }
            .padding(.horizontal,22).padding(.vertical,10)
        }
        .background(Theme.ink.opacity(0.92))
    }
    @ViewBuilder private func content(_ size:CGSize)->some View {
        switch page {
        case .home,.game:
            stage(size)
        case .graphics:
            settingsGroup("01","Preset & resolution") {
                LauncherSettingPicker(title:"Graphics Preset",id:"preset",selection:Binding(get:{s.preset},set:{s.applyPreset($0)}),options:RebirthSettings.presets.map{($0,$0)})
                LauncherSettingPicker(title:"Resolution",id:"resolution",selection:Binding(get:{"\(s.width)x\(s.height)"},set:{let v=$0.split(separator:"x");s.width=Int(v[0])!;s.height=Int(v[1])!;s.preset="Custom"}),options:["640x360","854x480","960x540","1280x720"].map{($0,$0.replacingOccurrences(of:"x",with:" × "))},savedTitle:"\(s.width) × \(s.height)")
                choice("Resolution Scale","scale",\RebirthSettings.scale,[(50,"50%"),(70,"70%"),(100,"100%")],savedTitle:"\(s.scale)%")
            }
            Text("Internal rendering: \(s.resolution). Display and touch coordinates remain independent.").font(.caption).foregroundStyle(Theme.muted)
            settingsGroup("02","Performance") {
                choice("Frame Rate Limit","fps",\RebirthSettings.fps,[(15,"15 FPS"),(20,"20 FPS"),(30,"30 FPS"),(60,"60 FPS")])
            }
            settingsGroup("03","Image quality") {
                choice("Texture Quality","texture",\RebirthSettings.texture,[(0,"Normal"),(1,"High"),(2,"Very High")])
                settingToggle("FXAA","fxaa",edit(\RebirthSettings.fxaa))
                choice("Shadow Quality","shadow",\RebirthSettings.shadow,[(-1,"Off"),(0,"Normal"),(1,"High"),(2,"Very High")])
                quality("Reflection Quality","reflection",\RebirthSettings.reflection,3);quality("Water Quality","water",\RebirthSettings.water,1);quality("Particle Quality","particles",\RebirthSettings.particles,2);quality("Grass Quality","grass",\RebirthSettings.grass,3);quality("Post Processing","postfx",\RebirthSettings.postfx,3);quality("Ambient Occlusion","ssao",\RebirthSettings.ssao,2)
                choice("Anisotropic Filtering","anisotropy",\RebirthSettings.anisotropy,[(0,"Off"),(2,"2×"),(4,"4×"),(8,"8×"),(16,"16×")])
            }
            settingsGroup("04","Overlay") {
                settingToggle("Frame Statistics","statistics",$s.show_fps)
            }
            card("Unavailable pending validation","MSAA, frame rates above 60 or uncapped rendering, native-display resolution and dynamic upscaling are not enabled. All graphics changes apply at the next clean launch.")
            Color.clear.frame(height:64).accessibilityHidden(true)
        case .gameplay:slider("Ambient population",edit(\RebirthSettings.density),0...1);card("Separate progression","Story and Sandbox receive separate byte-verified Rebirth save copies. Legacy saves remain untouched for rollback. No save payload conversion is performed.")
        case .controls:slider("Touch look sensitivity",$s.touch_sensitivity,0.25...2);slider("Controller look sensitivity",$s.controller_sensitivity,0.25...2);card("Muguet controls","Native GameController and touch input remain upstream-owned. Additional deadzone, layout, invert-axis and switching preferences need integration testing before being exposed.")
        case .audio:slider("Master volume",$s.volume,0...1);Toggle("Mute",isOn:$s.mute).padding(14).panel();card("Native audio","Gain is applied to actual 48 kHz stereo PCM at the CoreAudio output. Muting still drains the ring. Dialogue/radio/effects bus controls are unavailable. Speaker playback requires device verification.")
        case .files:folderCard;card("Files stay where they are","Select your existing web folder. Archives and shader data are read in place; no ZIP import, download or replacement. The launcher validates the pinned original shader build before starting.")
        case .advanced:card("Execution architecture","Muguet Rust host, Wasmtime AOT and wgpu 30 Metal. This fork does not contain selectable legacy wasm2c, Renderer V2 or CPU Core V2 paths.");card("Runtime","Muguet · Wasmtime · Rust wgpu / Metal. Previous native renderers and CPU cores are not linked into this fork.");card("Offline","Multiplayer is disabled for this migration milestone.");Button("Reset Settings"){s=RebirthSettings();loaded=true;error=nil;try? RebirthStore.save(s)}.buttonStyle(.bordered)
        case .diagnostics:memoryCard;card(Self.installed.summary,"Source: \(Self.installed.source)\nApp: \(Self.installed.bundle)");Toggle("Display diagnosis (temporary magenta test)",isOn:$displayProbe).padding(14).panel();Text("On next launch: 30 test presents, then eight tiny pixel samples. Off by default; does not prove gameplay or visible output.").font(.caption).foregroundStyle(Theme.muted);card("Validation status","Source and host tests do not prove iPhone gameplay. Exact upstream/source revisions and validation results accompany each build.");card("Session reports","Share a TXT snapshot of Muguet's engine and runtime logs. Retention is bounded at 256 MiB across launches; older text is rotated. Original game assets are excluded.");Button(exporting ? "Preparing report…" : "Share TXT Report",action:shareLogs).buttonStyle(.bordered).disabled(exporting);Text("Upstream 1b4600b · Rebirth development milestone").font(.caption.monospaced()).foregroundStyle(Theme.muted)
        }
    }
    // Home: a stage over the full-bleed picture. Text sits bottom-left and the column is
    // capped so the faces in the Story picture stay clear.
    private func stage(_ size:CGSize)->some View {
        let column=min(400,size.width*0.46)
        return VStack(alignment:.leading,spacing:14) {
            Spacer(minLength:0)
            VStack(alignment:.leading,spacing:8) {
                Text(s.mode==1 ? "THE ORIGINAL CAMPAIGN":"OFFLINE OPEN WORLD").font(.caption.weight(.bold)).kerning(2.4).foregroundStyle(Color.accentColor)
                Text(s.mode==1 ? "STORY MODE":"SANDBOX MODE").font(.largeTitle.weight(.black).italic()).foregroundStyle(.white).fixedSize(horizontal:false,vertical:true)
                Text(s.mode==1 ? "Three lives, one city. Your progress is saved separately from Sandbox." : "Free roam with its own saves. No story required.").font(.subheadline).foregroundStyle(Color.white.opacity(0.82)).fixedSize(horizontal:false,vertical:true)
            }
            .id("stage-text-\(s.mode)")
            .transition(.asymmetric(insertion:AnyTransition.opacity.combined(with:.offset(x:0,y:12)),removal:.opacity))
            HStack(spacing:12) {
                ModeTile(mode:1,selected:s.mode==1){choose(mode:1)}
                ModeTile(mode:2,selected:s.mode==2){choose(mode:2)}
            }.reveal(0.1)
            ScrollView(.horizontal,showsIndicators:false) {
                HStack(spacing:10) {
                    chip("GAME FOLDER",folder.url?.lastPathComponent ?? "Not selected"){picker=true}
                    chip("MEMORY",memory.shortLabel){select(.diagnostics)}
                    chip("BUILD",Self.installed.summary){select(.diagnostics)}.accessibilityIdentifier("rebirth.installedVersion")
                }
            }.reveal(0.2)
            if !hasGameCode() {Text("Runtime validation build: no embedded game module, so PLAY stays unavailable.").font(.caption).foregroundStyle(Theme.warn).fixedSize(horizontal:false,vertical:true)}
        }
        .frame(width:column,alignment:.leading)
        .frame(maxWidth:.infinity,minHeight:max(240,size.height-150),alignment:.bottomLeading)
    }
    private func chip(_ label:String,_ value:String,action:@escaping()->Void)->some View {
        Button(action:action) {
            VStack(alignment:.leading,spacing:2) {
                Text(label).font(.caption2.weight(.bold)).kerning(1.6).foregroundStyle(Theme.muted)
                Text(value).font(.footnote.weight(.semibold)).lineLimit(1).foregroundStyle(.white)
            }
            .padding(.horizontal,12).padding(.vertical,6).frame(minHeight:44,alignment:.leading)
            .background(Theme.surface.opacity(0.88),in:RoundedRectangle(cornerRadius:4))
            .overlay(RoundedRectangle(cornerRadius:4).stroke(Theme.line,lineWidth:1))
        }
        .buttonStyle(PressStyle())
    }
    private var memoryCard:some View {
        VStack(alignment:.leading,spacing:10) {
            HStack {
                Text("MEMORY ENTITLEMENT").font(.caption.weight(.bold)).kerning(1.8).foregroundStyle(Color.accentColor)
                Spacer()
                Circle().fill(memory.tone).frame(width:10,height:10).accessibilityHidden(true)
            }
            Text(memory.title).font(.headline).fixedSize(horizontal:false,vertical:true)
            memoryRow("Signing profile",memory.profileText)
            memoryRow("Memory ceiling",memory.ceilingText)
            memoryRow("Device memory",memory.deviceText)
            Text("Recommended: keep Increased Memory Limit active. GTA V needs far more memory than iOS allows by default, and without it the system is much more likely to close the app. This app already asks for the entitlement (plus Extended Virtual Addressing), but iOS only honours it if the profile you sign with grants it. Free Apple ID signing usually drops it; a paid Apple Developer account with the Increased Memory Limit capability on this app's ID keeps it.").font(.subheadline).foregroundStyle(Theme.muted).fixedSize(horizontal:false,vertical:true)
            Text("The ceiling is measured as the app's current footprint plus the memory iOS reports as still available. It is a live reading, not a guarantee.").font(.caption).foregroundStyle(Theme.muted).fixedSize(horizontal:false,vertical:true)
            Button("Measure again"){memory=MemoryStatus.measure()}.buttonStyle(.bordered)
        }
        .padding(16).frame(maxWidth:.infinity,alignment:.leading).panel()
    }
    private func memoryRow(_ name:String,_ value:String)->some View {
        HStack(alignment:.firstTextBaseline) {
            Text(name).font(.subheadline).foregroundStyle(Theme.muted)
            Spacer(minLength:12)
            Text(value).font(.subheadline.weight(.semibold)).multilineTextAlignment(.trailing).fixedSize(horizontal:false,vertical:true)
        }
    }
    private var folderCard:some View{VStack(alignment:.leading,spacing:8){Text("GAME FILES").font(.caption.weight(.bold)).kerning(1.8).foregroundStyle(Color.accentColor);Text(folder.url?.lastPathComponent ?? "No folder selected").font(.headline);if let e=folder.error{Text(e).foregroundStyle(Theme.bad)};Text("Permission restored does not imply all assets are present.").font(.caption).foregroundStyle(Theme.muted);Button("Select / Change Folder"){picker=true}.buttonStyle(.bordered)}.frame(maxWidth:.infinity,alignment:.leading).padding(16).panel()}
    private func card(_ title:String,_ text:String)->some View{VStack(alignment:.leading,spacing:7){Text(title).font(.headline);Text(text).font(.subheadline).foregroundStyle(Theme.muted).fixedSize(horizontal:false,vertical:true)}.frame(maxWidth:.infinity,alignment:.leading).padding(16).panel()}
    private func settingsGroup<Rows:View>(_ index:String,_ title:String,@ViewBuilder _ rows:()->Rows)->some View {
        VStack(alignment:.leading,spacing:10) {
            SectionHeader(index:index,title:title)
            VStack(spacing:0){rows()}.panel()
        }
    }
    private func edit<T>(_ key:WritableKeyPath<RebirthSettings,T>)->Binding<T>{Binding(get:{s[keyPath:key]},set:{s[keyPath:key]=$0;s.preset="Custom"})}
    private func choice(_ title:String,_ id:String,_ key:WritableKeyPath<RebirthSettings,Int>,_ values:[(Int,String)],savedTitle:String?=nil)->some View{LauncherSettingPicker(title:title,id:id,selection:edit(key),options:values,savedTitle:savedTitle)}
    private func quality(_ title:String,_ id:String,_ key:WritableKeyPath<RebirthSettings,Int>,_ maximum:Int)->some View{choice(title,id,key,(0...maximum).map{($0,["Normal","High","Very High","Ultra"][$0])})}
    private func settingToggle(_ title:String,_ id:String,_ value:Binding<Bool>)->some View {
        LauncherSettingRow(title:title,id:id) {
            HStack(spacing:8) {
                Text(value.wrappedValue ? "On":"Off").foregroundStyle(value.wrappedValue ? Color.accentColor:Theme.muted).fixedSize(horizontal:false,vertical:true)
                Toggle(title,isOn:value).labelsHidden().fixedSize()
                    .accessibilityLabel(title).accessibilityIdentifier("graphics.\(id).control")
            }
        }
    }
    private func slider(_ title:String,_ value:Binding<Double>,_ range:ClosedRange<Double>)->some View{VStack(alignment:.leading,spacing:8){HStack{Text(title);Spacer();Text(String(format:"%.2f",value.wrappedValue)).font(.body.monospacedDigit()).foregroundStyle(Color.accentColor)};Slider(value:value,in:range)}.padding(14).panel()}
    private func shareLogs(){
        guard !exporting else{return};exporting=true
        let root=FileManager.default.urls(for:.applicationSupportDirectory,in:.userDomainMask)[0].appendingPathComponent("RebirthExports").path
        DispatchQueue.global(qos:.utility).async {
            let path=root.withCString{exportLogs($0)}.map{String(cString:$0)}
            let message=path == nil ? launchError().map{String(cString:$0)} : nil
            DispatchQueue.main.async {exporting=false;if let path{sharedLog=SharedLog(url:URL(fileURLWithPath:path))}else{error=message ?? "Report export failed"}}
        }
    }
    private func start(){
        guard loaded,let root=folder.url,hasGameCode() else{return}
        do {
            try RebirthStore.save(s)
            let json=String(decoding:try JSONEncoder().encode(s),as:UTF8.self)
            let support=FileManager.default.urls(for:.applicationSupportDirectory,in:.userDomainMask)[0]
            try FileManager.default.createDirectory(at:support,withIntermediateDirectories:true)
            starting=true
            let rootPath=root.path,supportPath=support.path
            DispatchQueue.global(qos:.userInitiated).async {
                let ok=rootPath.withCString{r in json.withCString{j in supportPath.withCString{p in prepareLaunch(r,j,p)}}}
                let message=ok ? nil : (launchError().map{String(cString:$0)} ?? "Launch validation failed")
                DispatchQueue.main.async {starting=false;if ok{play()}else{error=message}}
            }
        }catch{self.error=error.localizedDescription}
    }
}

private struct SharedLog:Identifiable {let id=UUID();let url:URL}
private struct LogShare:UIViewControllerRepresentable {
    let url:URL
    func makeUIViewController(context:Context)->UIActivityViewController {UIActivityViewController(activityItems:[url],applicationActivities:nil)}
    func updateUIViewController(_ controller:UIActivityViewController,context:Context){}
}
