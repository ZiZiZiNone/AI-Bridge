package com.aibridge.mobile.ui

import androidx.compose.foundation.layout.padding
import androidx.compose.material3.NavigationBar
import androidx.compose.material3.NavigationBarItem
import androidx.compose.material3.Scaffold
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import com.aibridge.mobile.AppDependencies
import com.aibridge.mobile.ui.pages.ContainerBrowserScreen
import com.aibridge.mobile.ui.pages.GeneratorScreen
import com.aibridge.mobile.ui.pages.PairingScreen
import com.aibridge.mobile.ui.pages.SettingsBackupScreen
import com.aibridge.mobile.ui.pages.VaultListScreen

/**
 * M2 主屏骨架：底部五 tab（库 / 生成器 / 浏览器 / 设备 / 设置）。
 *
 * 页面归属：库 → M2（M3 由其内导航进入）· 生成器 → M4 · 浏览器 → M5（M6 为其次级页面）·
 *           设备 → M7 · 设置 → M8 · M1 为前置锁屏，M9 由系统服务自带形态（不经本 Composable）。
 * 薄入口：本 Composable 只做页面装配与依赖传递，无业务判断。
 */
@Composable
fun RootNav(dependencies: AppDependencies) {
    var selectedTab by remember { mutableStateOf(0) }
    val tabLabels = listOf("库", "生成器", "浏览器", "设备", "设置")

    Scaffold(
        bottomBar = {
            NavigationBar {
                tabLabels.forEachIndexed { index, label ->
                    NavigationBarItem(
                        selected = selectedTab == index,
                        onClick = { selectedTab = index },
                        icon = { Text(text = label) },
                    )
                }
            }
        },
    ) { innerPadding ->
        val contentModifier = Modifier.padding(innerPadding)
        when (selectedTab) {
            0 -> VaultListScreen(dependencies = dependencies, modifier = contentModifier)
            1 -> GeneratorScreen(dependencies = dependencies, modifier = contentModifier)
            2 -> ContainerBrowserScreen(dependencies = dependencies, modifier = contentModifier)
            3 -> PairingScreen(dependencies = dependencies, modifier = contentModifier)
            else -> SettingsBackupScreen(dependencies = dependencies, modifier = contentModifier)
        }
    }
}
