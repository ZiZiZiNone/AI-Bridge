import AuthenticationServices
import SwiftUI

/// M9 / M-F3：iOS Credential Provider Extension（系统自动填充 UI）。
///
/// 职责边界（薄入口）：
/// - 只做「系统请求 → core 候选 → ASCredential」的系统适配与类型翻译；
/// - 解锁判定、账号匹配、去重语义全部在 core；
/// - 仅账号类条目（账号密码 / passkey / TOTP），不设支付信息 / 身份分类。
/// 装配：本扩展由系统实例化；依赖在本实例内构造注入（`lazy` 为实例级延迟，非全局状态）。
/// 解锁语义（M-F3）：库锁定时请求用户交互（进解锁 UI）；解锁后选账号填入，用完即走。
final class CredentialProviderViewController: ASCredentialProviderViewController {

    private lazy var environment: AppEnvironment = {
        do {
            return try bootstrapAppEnvironment()
        } catch {
            fatalError("Credential Provider 装配失败：\(error)")
        }
    }()

    private var selectedCredentialIdentity: ASPasswordCredentialIdentity?

    override func prepareCredentialList(for serviceIdentifiers: [ASCredentialServiceIdentifier]) {
        let query = serviceIdentifiers.first?.identifier ?? ""
        let listView = CredentialListView(environment: environment, serviceQuery: query) { [weak self] user, password in
            self?.completeWith(user: user, password: password)
        }
        let hosting = UIHostingController(rootView: listView)
        addChild(hosting)
        hosting.view.frame = view.bounds
        hosting.view.autoresizingMask = [.flexibleWidth, .flexibleHeight]
        view.addSubview(hosting.view)
        hosting.didMove(toParent: self)
    }

    override func provideCredentialWithoutUserInteraction(for credentialIdentity: ASPasswordCredentialIdentity) {
        guard !environment.core.vault().isLocked() else {
            // 锁定态不静默降级：要求用户交互解锁（M-F3）。
            cancelRequest(withError: ASExtensionError.userInteractionRequired as NSError)
            return
        }
        // recordIdentifier 即 core 的 account_ref（注册 Identity 时写入；命名待绑定生成后校准）。
        let accountRef = credentialIdentity.recordIdentifier ?? credentialIdentity.user
        do {
            let entry = try environment.core.vault().getEntry(accountRef: accountRef)
            completeWith(user: entry.username ?? "", password: entry.secret ?? "")
        } catch {
            cancelRequest(withError: ASExtensionError.credentialIdentityNotFound as NSError)
        }
    }

    private func completeWith(user: String, password: String) {
        let credential = ASPasswordCredential(user: user, password: password)
        completeRequest(withSelectedCredential: credential) { [weak self] expired in
            if expired {
                self?.cancelRequest(withError: ASExtensionError.requestFailed as NSError)
            }
        }
    }
}
