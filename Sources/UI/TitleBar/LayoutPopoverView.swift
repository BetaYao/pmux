import AppKit

enum DashboardLayout: String, CaseIterable {
    case grid = "grid"
    case leftRight = "left-right"
    case topSmall = "top-small"
    case topLarge = "top-large"

    var displayName: String {
        switch self {
        case .grid: return "Grid"
        case .leftRight: return "Left-Right"
        case .topSmall: return "Top-Small"
        case .topLarge: return "Top-Large"
        }
    }

    var iconName: String {
        switch self {
        case .grid: return "square.grid.2x2"
        case .leftRight: return "rectangle.lefthalf.filled"
        case .topSmall: return "rectangle.tophalf.inset.filled"
        case .topLarge: return "rectangle.bottomhalf.inset.filled"
        }
    }
}
