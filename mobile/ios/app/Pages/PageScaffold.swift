import SwiftUI

/// 页面脚手架：呈现界面编号锚点、名称与承载功能点，便于与 docs/功能点全景.md §3.3 逐条对照。
/// 约束：不含业务判断、不做数据转换；内容由各页面组装（薄入口）。
struct PageScaffold<Content: View>: View {
    let screenId: String
    let title: String
    let functionPoints: String
    private let content: Content

    init(
        screenId: String,
        title: String,
        functionPoints: String,
        @ViewBuilder content: () -> Content
    ) {
        self.screenId = screenId
        self.title = title
        self.functionPoints = functionPoints
        self.content = content()
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 8) {
            VStack(alignment: .leading, spacing: 2) {
                Text(screenId)
                    .font(.caption)
                    .foregroundStyle(.secondary)
                Text(title)
                    .font(.headline)
                Text("承载功能点：\(functionPoints)")
                    .font(.caption2)
                    .foregroundStyle(.secondary)
            }
            content
        }
    }
}
