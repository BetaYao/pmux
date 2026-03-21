import XCTest
@testable import pmux

final class AgentTypeTests: XCTestCase {

    // MARK: - Detection from terminal content

    func testDetectClaudeCode() {
        let result = AgentType.detect(fromLowercased: "claude code v1.2.3 press esc to interrupt")
        XCTAssertEqual(result, .claudeCode)
    }

    func testDetectCodex() {
        let result = AgentType.detect(fromLowercased: "codex> running task")
        XCTAssertEqual(result, .codex)
    }

    func testDetectOpenCode() {
        let result = AgentType.detect(fromLowercased: "opencode v0.5.0 ready")
        XCTAssertEqual(result, .openCode)
    }

    func testDetectGemini() {
        let result = AgentType.detect(fromLowercased: "gemini cli v2.0")
        XCTAssertEqual(result, .gemini)
    }

    func testDetectCline() {
        let result = AgentType.detect(fromLowercased: "cline> working on task")
        XCTAssertEqual(result, .cline)
    }

    func testDetectGoose() {
        let result = AgentType.detect(fromLowercased: "goose session started")
        XCTAssertEqual(result, .goose)
    }

    func testDetectAider() {
        let result = AgentType.detect(fromLowercased: "aider v0.40 main branch")
        XCTAssertEqual(result, .aider)
    }

    func testDetectUnknown() {
        let result = AgentType.detect(fromLowercased: "bash-5.2$ ls -la")
        XCTAssertEqual(result, .unknown)
    }

    func testDetectEmpty() {
        let result = AgentType.detect(fromLowercased: "")
        XCTAssertEqual(result, .unknown)
    }

    // MARK: - Specificity ordering

    func testOpenCodeBeforeCode() {
        // "opencode" should match .openCode, not something else containing "code"
        let result = AgentType.detect(fromLowercased: "opencode session active")
        XCTAssertEqual(result, .openCode)
    }

    // MARK: - Display names

    func testDisplayNames() {
        XCTAssertEqual(AgentType.claudeCode.displayName, "Claude Code")
        XCTAssertEqual(AgentType.codex.displayName, "Codex")
        XCTAssertEqual(AgentType.openCode.displayName, "OpenCode")
        XCTAssertEqual(AgentType.unknown.displayName, "Unknown")
    }
}
