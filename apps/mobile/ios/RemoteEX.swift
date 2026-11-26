import UIKit
import WebKit

@main
final class AppDelegate: UIResponder, UIApplicationDelegate {
    func application(_ application: UIApplication,
                     configurationForConnecting session: UISceneSession,
                     options: UIScene.ConnectionOptions) -> UISceneConfiguration {
        let configuration = UISceneConfiguration(name: "Default Configuration", sessionRole: session.role)
        configuration.delegateClass = SceneDelegate.self
        return configuration
    }
}

final class SceneDelegate: UIResponder, UIWindowSceneDelegate {
    var window: UIWindow?
    private var controller: Controller?
    func scene(_ scene: UIScene, willConnectTo session: UISceneSession, options: UIScene.ConnectionOptions) {
        guard let windowScene = scene as? UIWindowScene else { return }
        let c = Controller()
        let w = UIWindow(windowScene: windowScene)
        w.rootViewController = c
        window = w
        controller = c
        w.makeKeyAndVisible()
    }
    func sceneWillResignActive(_ scene: UIScene) {
        // Close the foreground web session, do not keep displaying its last frame.
        controller?.stop("Session stopped when the app left the foreground.")
    }
    func sceneDidEnterBackground(_ scene: UIScene) {
        controller?.stop("Session stopped in background.")
    }
    func sceneDidDisconnect(_ scene: UIScene) {
        controller?.stop("Session closed.")
    }
}

struct ControllerOrigin {
    let host: String
    let origin: String
    static let paths: Set<String> = ["/", "/index.html", "/mobile.html", "/identity.html", "/view.html"]
    init?(_ value: String, domains: [String]) {
        guard value.count <= 512, let u = URLComponents(string: value),
              u.scheme == "https", let host = u.host, !host.isEmpty,
              host == host.lowercased(), host.contains("."),
              host.utf8.allSatisfy({ (97...122).contains($0) || (48...57).contains($0) || $0 == 45 || $0 == 46 }),
              u.port == nil, u.user == nil, u.password == nil, u.path.isEmpty,
              u.query == nil, u.fragment == nil, value == "https://" + host,
              domains == [host] else { return nil }
        self.host = host
        self.origin = value
    }
    func permits(_ url: URL?) -> Bool {
        guard let url = url, let u = URLComponents(url: url, resolvingAgainstBaseURL: false) else { return false }
        return u.scheme == "https" && u.host == host && u.port == nil &&
               u.user == nil && u.password == nil && u.query == nil && u.fragment == nil &&
               Self.paths.contains(u.percentEncodedPath.isEmpty ? "/" : u.percentEncodedPath)
    }
}

final class Controller: UIViewController, WKNavigationDelegate, WKUIDelegate {
    private let origin: ControllerOrigin? = {
        let bundle = Bundle.main
        return ControllerOrigin(bundle.object(forInfoDictionaryKey: "RemoteEXControllerOrigin") as? String ?? "",
                                domains: bundle.object(forInfoDictionaryKey: "WKAppBoundDomains") as? [String] ?? [])
    }()
    private var web: WKWebView?
    private let status = UILabel()
    private let openButton = UIButton(type: .system)
    private let stopButton = UIButton(type: .system)
    private let surface = UIView()
    override func viewDidLoad() {
        super.viewDidLoad()
        view.backgroundColor = .systemBackground
        let title = UILabel()
        title.text = "Remote EX"
        title.font = .preferredFont(forTextStyle: .title1)
        status.font = .preferredFont(forTextStyle: .body)
        status.numberOfLines = 0
        status.text = origin == nil ? "Offline candidate — no reviewed HTTPS controller configured." :
            "Reviewed controller: \(origin!.origin). Open explicitly. Device enrollment and host approval are separate."
        openButton.setTitle("Open controller", for: .normal)
        openButton.isEnabled = origin != nil
        openButton.addTarget(self, action: #selector(openController), for: .touchUpInside)
        stopButton.setTitle("Disconnect", for: .normal)
        stopButton.isEnabled = false
        stopButton.addTarget(self, action: #selector(disconnect), for: .touchUpInside)
        let buttons = UIStackView(arrangedSubviews: [openButton, stopButton])
        buttons.axis = .horizontal
        buttons.distribution = .fillEqually
        let stack = UIStackView(arrangedSubviews: [title, status, buttons, surface])
        stack.axis = .vertical
        stack.spacing = 12
        stack.translatesAutoresizingMaskIntoConstraints = false
        view.addSubview(stack)
        NSLayoutConstraint.activate([
            stack.leadingAnchor.constraint(equalTo: view.safeAreaLayoutGuide.leadingAnchor, constant: 16),
            stack.trailingAnchor.constraint(equalTo: view.safeAreaLayoutGuide.trailingAnchor, constant: -16),
            stack.topAnchor.constraint(equalTo: view.safeAreaLayoutGuide.topAnchor, constant: 12),
            stack.bottomAnchor.constraint(equalTo: view.safeAreaLayoutGuide.bottomAnchor, constant: -8),
            buttons.heightAnchor.constraint(greaterThanOrEqualToConstant: 44)
        ])
    }
    @objc private func openController() {
        guard web == nil, let origin = origin,
              let url = URL(string: origin.origin + "/mobile.html") else { return }
        let configuration = WKWebViewConfiguration()
        configuration.websiteDataStore = .default()
        configuration.limitsNavigationsToAppBoundDomains = true
        configuration.allowsInlineMediaPlayback = true
        configuration.mediaTypesRequiringUserActionForPlayback = []
        configuration.preferences.javaScriptCanOpenWindowsAutomatically = false
        let w = WKWebView(frame: .zero, configuration: configuration)
        w.navigationDelegate = self
        w.uiDelegate = self
        w.allowsBackForwardNavigationGestures = false
        w.translatesAutoresizingMaskIntoConstraints = false
        surface.addSubview(w)
        NSLayoutConstraint.activate([
            w.leadingAnchor.constraint(equalTo: surface.leadingAnchor),
            w.trailingAnchor.constraint(equalTo: surface.trailingAnchor),
            w.topAnchor.constraint(equalTo: surface.topAnchor),
            w.bottomAnchor.constraint(equalTo: surface.bottomAnchor)
        ])
        web = w
        openButton.isEnabled = false
        stopButton.isEnabled = true
        status.text = "Controller opened. No incoming phone control or background session."
        w.load(URLRequest(url: url, cachePolicy: .reloadIgnoringLocalCacheData))
    }
    @objc private func disconnect() { stop("Disconnected. Open manually to start again.") }
    func stop(_ message: String) {
        guard isViewLoaded else { return }
        if let w = web {
            web = nil
            w.stopLoading()
            w.navigationDelegate = nil
            w.uiDelegate = nil
            w.removeFromSuperview()
        }
        // WKWebsiteDataStore identity is retained. No automatic session restore.
        openButton.isEnabled = origin != nil
        stopButton.isEnabled = false
        status.text = message
    }
    func webView(_ webView: WKWebView, decidePolicyFor action: WKNavigationAction,
                 decisionHandler: @escaping (WKNavigationActionPolicy) -> Void) {
        let permitted = webView === web && action.targetFrame?.isMainFrame == true &&
            action.request.httpMethod == "GET" && origin?.permits(action.request.url) == true
        decisionHandler(permitted ? .allow : .cancel)
    }
    func webView(_ webView: WKWebView, decidePolicyFor response: WKNavigationResponse,
                 decisionHandler: @escaping (WKNavigationResponsePolicy) -> Void) {
        let permitted = webView === web && response.isForMainFrame && response.canShowMIMEType &&
            response.response.mimeType == "text/html" && origin?.permits(response.response.url) == true
        decisionHandler(permitted ? .allow : .cancel)
    }
    func webView(_ webView: WKWebView, createWebViewWith configuration: WKWebViewConfiguration,
                 for navigationAction: WKNavigationAction, windowFeatures: WKWindowFeatures) -> WKWebView? {
        return nil
    }
    func webView(_ webView: WKWebView, requestMediaCapturePermissionFor origin: WKSecurityOrigin,
                 initiatedByFrame frame: WKFrameInfo, type: WKMediaCaptureType,
                 decisionHandler: @escaping (WKPermissionDecision) -> Void) {
        decisionHandler(.deny)
    }
    func webViewWebContentProcessDidTerminate(_ webView: WKWebView) {
        if webView === web { stop("Web content stopped. Reopen manually.") }
    }
    func webView(_ webView: WKWebView, didFailProvisionalNavigation navigation: WKNavigation!, withError error: Error) {
        if webView === web { stop("Cannot load reviewed HTTPS controller. Check TLS and site configuration.") }
    }
}
