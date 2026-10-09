package com.aibridge.mobile

import android.os.Bundle
import androidx.activity.ComponentActivity
import androidx.activity.compose.setContent
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Surface
import com.aibridge.mobile.ui.RootNav

/**
 * AI-Bridge · Android 宿主入口（薄入口）。
 *
 * 职责：装配组合根并渲染根视图；不含任何业务判断、不做数据转换、不做状态计算。
 * 依赖方向：宿主 → core 公开面（经 mobile/ffi 生成的 UniFFI 绑定）；core 永不依赖本目录任何类型。
 * 依赖注入：一律构造注入并逐层显式传递；本仓库禁止全局单例 / 静态门面 / 服务定位器
 *           （不引入 Application 级依赖容器、不使用 Hilt 等定位器式框架）。
 */
class MainActivity : ComponentActivity() {

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        val dependencies: AppDependencies = bootstrapDependencies()
        setContent {
            MaterialTheme {
                Surface {
                    RootNav(dependencies = dependencies)
                }
            }
        }
    }

    /**
     * 组合根：本类内唯一的装配点（构造注入起点），装配结果只向下传递。
     *
     * 装配待 core `ab-ffi` 绑定生成 + Android 适配层（VaultStore / SessionSource / Transport /
     * Clock / Entropy 五个 core trait 的宿主实现）落地后接入，见 mobile/README.md「FFI 生成」。
     * 骨架阶段保持 fail-fast：不以占位依赖静默启动。
     */
    private fun bootstrapDependencies(): AppDependencies =
        TODO("AppDependencies 装配待 mobile/ffi 绑定生成后接入（见 mobile/README.md）")
}
