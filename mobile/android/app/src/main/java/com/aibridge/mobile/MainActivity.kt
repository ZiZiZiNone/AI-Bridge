package com.aibridge.mobile

import android.os.Bundle
import androidx.activity.ComponentActivity
import androidx.activity.compose.setContent
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Surface
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import com.aibridge.mobile.core.bootstrapAppDependencies
import com.aibridge.mobile.ui.RootNav
import com.aibridge.mobile.ui.pages.UnlockScreen

/**
 * AI-Bridge · Android 宿主入口（薄入口）+ 组合根装配点 + M1 锁屏门。
 *
 * 职责：装配组合根（bootstrapAppDependencies → AppDependencies）并渲染根视图；
 * 不含业务判断、不做数据转换、不做状态计算。
 * 依赖方向：宿主 → core 公开面（经 mobile/ffi 生成的 UniFFI 绑定）；core 永不依赖本目录任何类型。
 * 依赖注入：一律构造注入并逐层显式传递；本仓库禁止全局单例 / 静态门面 / 服务定位器
 *           （不引入 Application 级依赖容器、不使用 Hilt 等定位器式框架）。
 * 装配 fail-fast：绑定或宿主适配层缺失时 `bootstrapAppDependencies` 直接抛出，进程终止，
 *                不以占位依赖静默启动。
 */
class MainActivity : ComponentActivity() {

    private lateinit var dependencies: AppDependencies

    /** M1 锁屏门（S1/S2）：锁定态显示解锁屏，解锁后进入 M2 主结构。 */
    private var unlocked by mutableStateOf(false)

    /** S2 空闲自动锁：只覆盖前后台切换时长（与 iOS RootFlowView 同口径）。 */
    private var backgroundedAtMillis: Long? = null

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        dependencies = bootstrapAppDependencies(this)
        setContent {
            MaterialTheme {
                Surface {
                    if (unlocked) {
                        RootNav(dependencies = dependencies, onLock = ::lockVault)
                    } else {
                        UnlockScreen(dependencies = dependencies, onUnlock = { unlocked = true })
                    }
                }
            }
        }
    }

    override fun onStop() {
        super.onStop()
        backgroundedAtMillis = System.currentTimeMillis()
    }

    override fun onStart() {
        super.onStart()
        val since = backgroundedAtMillis ?: return
        backgroundedAtMillis = null
        if (System.currentTimeMillis() - since > dependencies.settings.autoLockSeconds * 1000L) {
            lockVault()
        }
    }

    private fun lockVault() {
        dependencies.core.vault().lock()
        unlocked = false
    }
}
