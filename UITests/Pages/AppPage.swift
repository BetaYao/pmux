import XCTest

/// Top-level page object for the pmux app.
class AppPage {
    let app: XCUIApplication

    init() {
        app = XCUIApplication()
    }

    @discardableResult
    func launch(testConfigPath: String? = nil) -> Self {
        if let path = testConfigPath {
            app.launchArguments += ["-UITestConfig", path]
        }
        app.launch()
        return self
    }

    func terminate() {
        app.terminate()
    }

    var dashboard: DashboardPage { DashboardPage(app) }
    var tabBar: TabBarPage { TabBarPage(app) }
    var sidebar: SidebarPage { SidebarPage(app) }
    var settings: SettingsPage { SettingsPage(app) }
    var dialog: DialogPage { DialogPage(app) }
    var repo: RepoPage { RepoPage(app) }
}
