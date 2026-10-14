import SwiftUI

/// M1 锁屏 / 解锁（S1；M-F1）。
///
/// 薄入口：解锁（Argon2id 派生等）与失败节流全在 core，本视图只收集口令并转发；
/// 生物识别仅作解锁因子（QuickUnlockStore 保存因子），主密码仍是唯一根。
struct UnlockView: View {
    let environment: AppEnvironment
    let onUnlock: () -> Void

    @State private var masterPassword = ""
    @State private var errorMessage: String?
    @State private var biometricAvailable = true

    var body: some View {
        PageScaffold(screenId: "M1", title: "锁屏 / 解锁", functionPoints: "S1") {
            VStack(spacing: 12) {
                SecureField("主密码", text: $masterPassword)
                    .textContentType(.password)
                    .textFieldStyle(.roundedBorder)

                Button("解锁") { unlockWithPassword() }
                    .buttonStyle(.borderedProminent)
                    .disabled(masterPassword.isEmpty)

                if biometricAvailable {
                    Button("生物识别解锁（仅解锁因子）") { unlockWithBiometrics() }
                        .buttonStyle(.bordered)
                }

                if let errorMessage {
                    Text(errorMessage)
                        .font(.footnote)
                        .foregroundStyle(.red)
                }

                Text("主密码是唯一根；生物识别仅作解锁因子。")
                    .font(.caption2)
                    .foregroundStyle(.secondary)

                Spacer()
            }
            .padding()
        }
        .navigationTitle("AI-Bridge")
    }

    private func unlockWithPassword() {
        do {
            try environment.core.vault().unlock(masterPassword: masterPassword)
            // 快速解锁因子仅在本设备 Keychain（生物门控）保存；保存失败不影响解锁。
            try? environment.quickUnlock.save(masterPassword: masterPassword)
            masterPassword = ""
            errorMessage = nil
            onUnlock()
        } catch {
            errorMessage = "解锁失败（\(error)）"
        }
    }

    private func unlockWithBiometrics() {
        do {
            let password = try environment.quickUnlock.retrieveMasterPassword()
            try environment.core.vault().unlock(masterPassword: password)
            errorMessage = nil
            onUnlock()
        } catch {
            biometricAvailable = false
            errorMessage = "快速解锁失败，请使用主密码（\(error)）"
        }
    }
}
