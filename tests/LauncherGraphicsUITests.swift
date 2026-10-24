import XCTest

final class LauncherGraphicsUITests:XCTestCase {
    private let rows:[(String,String)] = [
        ("preset","Graphics Preset"),("resolution","Resolution"),("scale","Resolution Scale"),
        ("fps","Frame Rate Limit"),("texture","Texture Quality"),("fxaa","FXAA"),
        ("shadow","Shadow Quality"),("reflection","Reflection Quality"),("water","Water Quality"),
        ("particles","Particle Quality"),("grass","Grass Quality"),("postfx","Post Processing"),
        ("ssao","Ambient Occlusion"),("anisotropy","Anisotropic Filtering"),("statistics","Frame Statistics")
    ]
    private var app:XCUIApplication!
    override func setUpWithError() throws {
        continueAfterFailure=false
        XCUIDevice.shared.orientation = .landscapeLeft
        app=XCUIApplication()
    }
    private func launch(_ fixture:String="high",_ type:String="large") {
        app.launchArguments=["Graphics","--fixture=\(fixture)","--type=\(type)"]
        app.launch()
        XCTAssertTrue(app.staticTexts["graphics.preset.label"].waitForExistence(timeout:10))
    }
    private var scroll:XCUIElement {app.scrollViews["launcher.content"]}
    private func control(_ id:String)->XCUIElement {
        app.descendants(matching:.any).matching(identifier:"graphics.\(id).control").firstMatch
    }
    private func reveal(_ id:String) {
        let label=app.staticTexts["graphics.\(id).label"]
        for _ in 0..<15 {
            if label.exists && control(id).exists {
                let visible=scroll.frame
                if visible.contains(label.frame) && visible.contains(control(id).frame) && label.isHittable && control(id).isHittable {return}
            }
            // Use fixed short scroll gestures: XCTest's synthesized velocity can
            // otherwise fling past rows on the compact SE accessibility layout.
            let bounds=label.frame.union(control(id).frame)
            let viewport=scroll.frame
            let delta:CGFloat = bounds.isEmpty || bounds.maxY>viewport.maxY ? 0.16 : -0.16
            let start=scroll.coordinate(withNormalizedOffset:CGVector(dx:0.5,dy:0.5+delta))
            let end=scroll.coordinate(withNormalizedOffset:CGVector(dx:0.5,dy:0.5-delta))
            start.press(forDuration:0.05,thenDragTo:end)
        }
        XCTFail("Could not fully expose setting \(id): \(label.frame), \(control(id).frame), viewport \(scroll.frame)")
    }
    private func checkRow(_ id:String,_ title:String) {
        reveal(id)
        let label=app.staticTexts["graphics.\(id).label"], item=control(id)
        XCTAssertEqual(label.label,title)
        XCTAssertGreaterThan(label.frame.width,0)
        XCTAssertGreaterThan(label.frame.height,0)
        XCTAssertLessThanOrEqual(label.frame.maxX,item.frame.minX+1,"Columns overlap: \(title)")
        XCTAssertTrue(app.frame.contains(label.frame),"Label out of screen: \(title)")
        XCTAssertTrue(app.frame.contains(item.frame),"Control out of screen: \(title)")
        XCTAssertTrue(item.isHittable)
    }
    private func screenshot(_ name:String) {
        let attachment=XCTAttachment(screenshot:app.screenshot());attachment.name=name;attachment.lifetime = .keepAlways;add(attachment)
    }
    func testAllLabelsAndLongValuesAtEveryTextSize() {
        for size in ["large","xxxLarge","accessibility5"] {
            launch("high",size)
            for (id,title) in rows {
                checkRow(id,title)
                if ["scale","shadow","anisotropy","statistics"].contains(id) {screenshot("graphics-\(size)-\(id)")}
            }
            app.terminate()
        }
    }
    private func choose(_ id:String,_ value:String) {
        reveal(id);control(id).tap()
        let option=app.buttons[value].firstMatch
        XCTAssertTrue(option.waitForExistence(timeout:3),"Missing choice \(value)")
        option.tap()
        let changed=XCTNSPredicateExpectation(predicate:NSPredicate(format:"value == %@",value),object:control(id))
        XCTAssertEqual(XCTWaiter.wait(for:[changed],timeout:3),.completed,"Selected value did not update")
    }
    func testChoicesCustomPresetTogglesAndPersistence() {
        launch()
        choose("preset","Ultra Potato")
        XCTAssertEqual(control("scale").value as? String,"50%")
        choose("resolution","1280 × 720")
        XCTAssertEqual(control("preset").value as? String,"Custom")
        for resolution in ["640 × 360","854 × 480","960 × 540","1280 × 720"] {choose("resolution",resolution)}
        choose("scale","70%")
        choose("fps","60 FPS")
        choose("texture","Very High")
        reveal("fxaa");let old=control("fxaa").value as? String;control("fxaa").tap()
        XCTAssertNotEqual(control("fxaa").value as? String,old)
        choose("shadow","Off")
        choose("reflection","Ultra")
        choose("water","High")
        choose("particles","Very High")
        choose("grass","Ultra")
        choose("postfx","Ultra")
        choose("ssao","Very High")
        choose("anisotropy","16×")
        reveal("statistics");control("statistics").tap()
        screenshot("graphics-settings-edited")
        app.terminate()
        app.launchArguments=["Graphics"];app.launch()
        XCTAssertTrue(app.staticTexts["graphics.preset.label"].waitForExistence(timeout:10))
        for (id,value) in [("preset","Custom"),("resolution","1280 × 720"),("scale","70%"),("fps","60 FPS"),("texture","Very High"),("shadow","Off"),("reflection","Ultra"),("anisotropy","16×")] {
            reveal(id);XCTAssertEqual(control(id).value as? String,value)
        }
    }
    func testMigratedScaleStillHasVisibleValue() {
        launch("legacy")
        XCTAssertEqual(app.staticTexts["graphics.scale.label"].label,"Resolution Scale")
        XCTAssertEqual(control("scale").value as? String,"60%")
        screenshot("graphics-migrated-60-percent")
    }
}
