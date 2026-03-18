import Foundation
import Vision

guard CommandLine.arguments.count > 1 else {
    fputs("Usage: ocr_tool <image_path>\n", stderr)
    exit(1)
}

let path = CommandLine.arguments[1]
guard FileManager.default.fileExists(atPath: path) else {
    fputs("Error: file not found: \(path)\n", stderr)
    exit(1)
}

guard let image = CGImage.from(path: path) else {
    fputs("Error: could not load image: \(path)\n", stderr)
    exit(2)
}

let semaphore = DispatchSemaphore(value: 0)
var recognizedText: [String] = []
var ocrError: Error?

let request = VNRecognizeTextRequest { request, error in
    if let error = error {
        ocrError = error
    } else if let observations = request.results as? [VNRecognizedTextObservation] {
        for observation in observations {
            if let candidate = observation.topCandidates(1).first {
                recognizedText.append(candidate.string)
            }
        }
    }
    semaphore.signal()
}

request.recognitionLevel = .accurate
request.recognitionLanguages = ["en-US", "zh-Hans"]
request.usesLanguageCorrection = true

let handler = VNImageRequestHandler(cgImage: image, options: [:])
do {
    try handler.perform([request])
} catch {
    fputs("Error: OCR failed: \(error)\n", stderr)
    exit(2)
}

semaphore.wait()

if let error = ocrError {
    fputs("Error: OCR failed: \(error)\n", stderr)
    exit(2)
}

for line in recognizedText {
    print(line)
}

// Helper extension to load CGImage from file path
extension CGImage {
    static func from(path: String) -> CGImage? {
        let url = URL(fileURLWithPath: path)
        guard let dataProvider = CGDataProvider(url: url as CFURL) else { return nil }
        let lowercasePath = path.lowercased()
        if lowercasePath.hasSuffix(".png") {
            return CGImage(pngDataProviderSource: dataProvider, decode: nil, shouldInterpolate: true, intent: .defaultIntent)
        } else if lowercasePath.hasSuffix(".jpg") || lowercasePath.hasSuffix(".jpeg") {
            return CGImage(jpegDataProviderSource: dataProvider, decode: nil, shouldInterpolate: true, intent: .defaultIntent)
        }
        // Fallback: try using ImageIO for other formats
        guard let source = CGImageSourceCreateWithDataProvider(dataProvider, nil) else { return nil }
        return CGImageSourceCreateImageAtIndex(source, 0, nil)
    }
}
