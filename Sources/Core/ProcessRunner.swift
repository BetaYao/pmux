import Foundation

enum ProcessRunner {
    /// Check if a command exists on PATH using login shell
    static func commandExists(_ command: String) -> Bool {
        let process = Process()
        process.executableURL = URL(fileURLWithPath: "/usr/bin/env")
        process.arguments = ["bash", "-lc", "command -v \(command)"]
        process.standardOutput = Pipe()
        process.standardError = Pipe()
        do {
            try process.run()
            process.waitUntilExit()
            return process.terminationStatus == 0
        } catch {
            return false
        }
    }

    /// Run a command and return trimmed stdout, or nil on failure
    static func output(_ args: [String]) -> String? {
        let process = Process()
        process.executableURL = URL(fileURLWithPath: "/usr/bin/env")
        process.arguments = args
        let pipe = Pipe()
        process.standardOutput = pipe
        process.standardError = Pipe()
        do {
            try process.run()
            process.waitUntilExit()
            guard process.terminationStatus == 0 else { return nil }
            let data = pipe.fileHandleForReading.readDataToEndOfFile()
            guard let str = String(data: data, encoding: .utf8) else { return nil }
            return str.trimmingCharacters(in: .whitespacesAndNewlines)
        } catch {
            return nil
        }
    }

    /// Run a command, ignoring output. Logs errors.
    static func runFireAndForget(_ args: [String]) {
        let process = Process()
        process.executableURL = URL(fileURLWithPath: "/usr/bin/env")
        process.arguments = args
        process.standardOutput = Pipe()
        process.standardError = Pipe()
        do {
            try process.run()
        } catch {
            NSLog("ProcessRunner: failed to run \(args.first ?? "?"): \(error)")
        }
    }

    /// Check if a command exists on PATH, calling back on the main queue.
    static func commandExistsAsync(_ command: String, completion: @escaping (Bool) -> Void) {
        DispatchQueue.global(qos: .userInitiated).async {
            let result = commandExists(command)
            DispatchQueue.main.async { completion(result) }
        }
    }

    /// Run a command and return trimmed stdout via callback on the main queue.
    static func outputAsync(_ args: [String], completion: @escaping (String?) -> Void) {
        DispatchQueue.global(qos: .userInitiated).async {
            let result = output(args)
            DispatchQueue.main.async { completion(result) }
        }
    }

    /// Run a command synchronously, waiting for exit. Logs errors.
    static func runSync(_ args: [String]) {
        let process = Process()
        process.executableURL = URL(fileURLWithPath: "/usr/bin/env")
        process.arguments = args
        process.standardOutput = Pipe()
        process.standardError = Pipe()
        do {
            try process.run()
            process.waitUntilExit()
        } catch {
            NSLog("ProcessRunner: failed to run \(args.first ?? "?"): \(error)")
        }
    }
}
