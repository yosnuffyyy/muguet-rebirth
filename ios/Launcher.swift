// muguet - Launcher.swift
// Constantin Clerc (c22dev) - 0.1

import SwiftUI
import UIKit
import UniformTypeIdentifiers
import os
import Metal

func memoryLimit() -> UInt64 {
    var info = task_vm_info_data_t()
    var count = mach_msg_type_number_t(MemoryLayout<task_vm_info_data_t>.size / MemoryLayout<natural_t>.size)
    let kr = withUnsafeMutablePointer(to: &info) {
        $0.withMemoryRebound(to: integer_t.self, capacity: Int(count)) { task_info(mach_task_self_, task_flavor_t(TASK_VM_INFO), $0, &count) }
    }
    return UInt64(os_proc_available_memory()) + (kr == KERN_SUCCESS ? info.phys_footprint : 0)
}

func hasMemoryEntitlement() -> Bool? {
    typealias Create = @convention(c) (CFAllocator?) -> Unmanaged<AnyObject>?
    typealias Copy = @convention(c) (AnyObject, CFString, UnsafeMutableRawPointer?) -> Unmanaged<AnyObject>?
    guard let sec = dlopen("/System/Library/Frameworks/Security.framework/Security", RTLD_NOW),
          let create = dlsym(sec, "SecTaskCreateFromSelf"), let copy = dlsym(sec, "SecTaskCopyValueForEntitlement"),
          let task = unsafeBitCast(create, to: Create.self)(nil)?.takeRetainedValue() else { return nil }
    let value = unsafeBitCast(copy, to: Copy.self)(task, "com.apple.developer.kernel.increased-memory-limit" as CFString, nil)?.takeRetainedValue()
    return (value as? Bool) ?? false
}

@_cdecl("muguet_memory_limit")
public func muguetMemoryLimit() -> UInt64 { memoryLimit() }

@_cdecl("muguet_memory_entitled")
public func muguetMemoryEntitled() -> Int32 { hasMemoryEntitlement().map { $0 ? 1 : 0 } ?? -1 }

final class RefreshRate: NSObject {
    static let shared = RefreshRate()
    private var link: CADisplayLink?

    @objc private func tick() {}

    func hold(_ hz: Float) {
        link?.invalidate()
        let l = CADisplayLink(target: self, selector: #selector(tick))
        l.preferredFrameRateRange = CAFrameRateRange(minimum: hz, maximum: hz, preferred: hz)
        l.add(to: .main, forMode: .common)
        link = l
    }
}

@_cdecl("muguet_hold_refresh")
public func muguetHoldRefresh(_ hz: Float) {
    RefreshRate.shared.hold(hz)
}

@_cdecl("muguet_show_launcher")
public func muguetShowLauncher(_ viewController: UnsafeMutableRawPointer, _ done: @convention(c) () -> Void) {
    let parent = Unmanaged<UIViewController>.fromOpaque(viewController).takeUnretainedValue()
    weak var sheet: UIViewController?
    let host = UIHostingController(rootView: LauncherView(play: {
        sheet?.dismiss(animated: true)
        done()
    }))
    sheet = host
    host.modalPresentationStyle = .fullScreen
    host.isModalInPresentation = true
    parent.present(host, animated: false)
}
