import XCTest
@testable import amux

class ProcessRunnerTests: XCTestCase {
    func testCommandExistsForKnownCommand() {
        XCTAssertTrue(ProcessRunner.commandExists("ls"))
    }

    func testCommandExistsForUnknownCommand() {
        XCTAssertFalse(ProcessRunner.commandExists("definitely_not_a_real_command_12345"))
    }

    func testOutputReturnsResult() {
        let output = ProcessRunner.output(["echo", "hello"])
        XCTAssertEqual(output, "hello")
    }

    func testOutputReturnsNilOnFailure() {
        let output = ProcessRunner.output(["false"])
        XCTAssertNil(output)
    }
}
