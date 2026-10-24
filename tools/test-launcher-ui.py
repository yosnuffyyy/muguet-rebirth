"""Run real SwiftUI interaction/layout tests; no Rust runtime or game assets.

The disposable Xcode project uses the production launcher/settings sources and
the same preview-only host stubs as preview-ios.py. No extra package dependency.
"""
import json
import pathlib
import plistlib
import shutil
import subprocess
import xml.etree.ElementTree as ET

ROOT = pathlib.Path(__file__).resolve().parent.parent
WORK = ROOT / 'build/launcher-ui-tests'
EVIDENCE = ROOT / 'evidence/local'
WORK.mkdir(parents=True, exist_ok=True)
EVIDENCE.mkdir(parents=True, exist_ok=True)

def output(*args):
    return subprocess.check_output(args, text=True).strip()

objects = {}
def obj(kind, **fields):
    key = f'{len(objects)+1:024X}'
    objects[key] = dict(isa=kind, **fields)
    return key

def config_list(settings):
    config = obj('XCBuildConfiguration', name='Debug', buildSettings=settings)
    return obj('XCConfigurationList', buildConfigurations=[config], defaultConfigurationIsVisible=0, defaultConfigurationName='Debug')

def sources(paths):
    refs = [obj('PBXFileReference', lastKnownFileType='sourcecode.swift', path=str(ROOT / p), sourceTree='<absolute>') for p in paths]
    builds = [obj('PBXBuildFile', fileRef=ref) for ref in refs]
    phase = obj('PBXSourcesBuildPhase', buildActionMask=2147483647, files=builds, runOnlyForDeploymentPostprocessing=0)
    return refs, phase

production = plistlib.loads((ROOT / 'ios/Info.plist').read_bytes())
info = dict(CFBundleIdentifier='$(PRODUCT_BUNDLE_IDENTIFIER)', CFBundleExecutable='$(EXECUTABLE_NAME)', CFBundleName='LauncherPreview', CFBundlePackageType='APPL', CFBundleVersion=production['CFBundleVersion'], CFBundleShortVersionString=production['CFBundleShortVersionString'], MinimumOSVersion='15.0', UIDeviceFamily=[1], UILaunchScreen={}, LSRequiresIPhoneOS=True, UISupportedInterfaceOrientations=['UIInterfaceOrientationLandscapeLeft','UIInterfaceOrientationLandscapeRight'], RebirthSourceRevision=output('git','rev-parse','HEAD'))
(WORK / 'Info.plist').write_bytes(plistlib.dumps(info))
common = dict(SWIFT_VERSION='5.0', IPHONEOS_DEPLOYMENT_TARGET='15.0', SDKROOT='iphonesimulator', SUPPORTED_PLATFORMS='iphonesimulator', TARGETED_DEVICE_FAMILY='1', CODE_SIGNING_ALLOWED='NO', ENABLE_TESTABILITY='YES', SWIFT_OPTIMIZATION_LEVEL='-Onone', CLANG_ENABLE_MODULES='YES')
app_refs, app_sources = sources(['ios/RebirthSettings.swift','ios/RebirthLauncher.swift','tests/LauncherPreview.swift'])
test_refs, test_sources = sources(['tests/LauncherGraphicsUITests.swift'])
app_product = obj('PBXFileReference', explicitFileType='wrapper.application', path='LauncherPreview.app', sourceTree='BUILT_PRODUCTS_DIR')
test_product = obj('PBXFileReference', explicitFileType='wrapper.cfbundle', path='LauncherUITests.xctest', sourceTree='BUILT_PRODUCTS_DIR')
app = obj('PBXNativeTarget', name='LauncherPreview', productName='LauncherPreview', productType='com.apple.product-type.application', productReference=app_product, buildPhases=[app_sources], buildRules=[], dependencies=[], buildConfigurationList=config_list(dict(common, PRODUCT_NAME='LauncherPreview', PRODUCT_BUNDLE_IDENTIFIER='io.github.yosnuffyyy.rebirth.uitest', INFOPLIST_FILE=str(WORK / 'Info.plist'))))
test = obj('PBXNativeTarget', name='LauncherUITests', productName='LauncherUITests', productType='com.apple.product-type.bundle.ui-testing', productReference=test_product, buildPhases=[test_sources], buildRules=[], dependencies=[], buildConfigurationList=config_list(dict(common, PRODUCT_NAME='LauncherUITests', PRODUCT_BUNDLE_IDENTIFIER='io.github.yosnuffyyy.rebirth.uitest.tests', GENERATE_INFOPLIST_FILE='YES', TEST_TARGET_NAME='LauncherPreview')))
products = obj('PBXGroup', children=[app_product,test_product], name='Products', sourceTree='<group>')
group = obj('PBXGroup', children=app_refs+test_refs+[products], sourceTree='<group>')
project = obj('PBXProject', attributes={'LastUpgradeCheck':'1600','TargetAttributes':{test:{'TestTargetID':app}}}, buildConfigurationList=config_list(common), compatibilityVersion='Xcode 14.0', developmentRegion='en', hasScannedForEncodings=0, knownRegions=['en','Base'], mainGroup=group, productRefGroup=products, projectDirPath='', projectRoot='', targets=[app,test])
proxy = obj('PBXContainerItemProxy', containerPortal=project, proxyType=1, remoteGlobalIDString=app, remoteInfo='LauncherPreview')
objects[test]['dependencies']=[obj('PBXTargetDependency', target=app, targetProxy=proxy)]
project_path=WORK / 'LauncherUI.xcodeproj'
project_path.mkdir(exist_ok=True)
(project_path / 'project.pbxproj').write_bytes(plistlib.dumps(dict(archiveVersion='1',classes={},objectVersion='56',objects=objects,rootObject=project)))
scheme = ET.Element('Scheme', LastUpgradeVersion='1600', version='1.3')
build = ET.SubElement(scheme,'BuildAction',parallelizeBuildables='YES',buildImplicitDependencies='YES')
entries = ET.SubElement(build,'BuildActionEntries')
def reference(parent, key, name, product_name):
    return ET.SubElement(parent,'BuildableReference',BuildableIdentifier='primary',BlueprintIdentifier=key,BuildableName=product_name,BlueprintName=name,ReferencedContainer='container:LauncherUI.xcodeproj')
for key,name,product_name in [(app,'LauncherPreview','LauncherPreview.app'),(test,'LauncherUITests','LauncherUITests.xctest')]:
    entry=ET.SubElement(entries,'BuildActionEntry',buildForTesting='YES',buildForRunning='YES',buildForProfiling='NO',buildForArchiving='NO',buildForAnalyzing='YES')
    reference(entry,key,name,product_name)
action=ET.SubElement(scheme,'TestAction',buildConfiguration='Debug',selectedDebuggerIdentifier='Xcode.DebuggerFoundation.Debugger.LLDB',selectedLauncherIdentifier='Xcode.IDEFoundation.Launcher.LLDB',shouldUseLaunchSchemeArgsEnv='YES')
testables=ET.SubElement(action,'Testables')
reference(ET.SubElement(testables,'TestableReference',skipped='NO'),test,'LauncherUITests','LauncherUITests.xctest')
scheme_dir=project_path / 'xcshareddata/xcschemes'
scheme_dir.mkdir(parents=True,exist_ok=True)
ET.ElementTree(scheme).write(scheme_dir / 'LauncherUI.xcscheme',encoding='utf-8',xml_declaration=True)

runtimes=json.loads(output('xcrun','simctl','list','runtimes','--json'))['runtimes']
runtime=next(r['identifier'] for r in reversed(runtimes) if r.get('isAvailable') and 'iOS' in r['name'])
device_types=json.loads(output('xcrun','simctl','list','devicetypes','--json'))['devicetypes']
results=[]
for suffix,device_name in [('small','iPhone SE (3rd generation)'),('large','iPhone 16 Pro Max')]:
    device_type=next(d['identifier'] for d in device_types if d['name']==device_name)
    device=output('xcrun','simctl','create',f'RebirthLabels-{suffix}',device_type,runtime)
    result_path=WORK / f'{suffix}.xcresult'
    try:
        command=['xcodebuild','test','-project',str(project_path),'-scheme','LauncherUI','-destination',f'platform=iOS Simulator,id={device}','-derivedDataPath',str(WORK / 'derived'),'-resultBundlePath',str(result_path),'-parallel-testing-enabled','NO','-maximum-concurrent-test-simulator-destinations','1','CODE_SIGNING_ALLOWED=NO']
        with (EVIDENCE / f'launcher-ui-{suffix}.log').open('w') as log:
            result=subprocess.run(command,stdout=log,stderr=subprocess.STDOUT,timeout=900)
        print((EVIDENCE / f'launcher-ui-{suffix}.log').read_text()[-18000:],flush=True)
        # Keep actual XCTest screenshots, including failed-test evidence.
        attachments=WORK / f'attachments-{suffix}'
        exported=subprocess.run(['xcrun','xcresulttool','export','attachments','--path',str(result_path),'--output-path',str(attachments)],capture_output=True,text=True)
        if exported.returncode==0:
            for index,file in enumerate(sorted(attachments.rglob('*.png'))):
                shutil.copyfile(file,EVIDENCE / f'launcher-ui-{suffix}-{index:03}.png')
            manifest=attachments / 'manifest.json'
            if manifest.exists(): shutil.copyfile(manifest,EVIDENCE / f'launcher-ui-{suffix}-screenshots.json')
        results.append(dict(device=device_name,runtime=runtime,exit_code=result.returncode,gameplay=False,productionRustLinked=False,sizes=['large','xxxLarge','accessibility5']))
        (EVIDENCE / 'launcher-ui-results.json').write_text(json.dumps(results,indent=2))
        if result.returncode: raise RuntimeError(f'Graphics UI regression failed on {device_name}; see evidence log')
    finally:
        subprocess.run(['xcrun','simctl','shutdown',device],capture_output=True)
        subprocess.run(['xcrun','simctl','delete',device],capture_output=True)
