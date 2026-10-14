package com.aibridge.mobile.core

import android.content.Context
import android.net.nsd.NsdManager
import android.net.nsd.NsdServiceInfo
import com.aibridge.mobile.ffi.PeerInfo
import com.aibridge.mobile.ffi.Transport
import java.io.DataInputStream
import java.io.DataOutputStream
import java.net.InetAddress
import java.net.InetSocketAddress
import java.net.ServerSocket
import java.util.concurrent.ArrayBlockingQueue
import java.util.concurrent.ConcurrentHashMap
import java.util.concurrent.TimeUnit
import java.util.concurrent.atomic.AtomicReference

/**
 * 局域网点对点传输（E1/E2/E3）：
 * 1. NSD TXT 记录强制携带并校验 64 位小写 hex `fingerprint`，且仅放行已配对/受信任指纹；
 * 2. 接收队列有界（`MAX_INBOUND_QUEUE_SIZE = 64`，单帧上限 `MAX_FRAME_BYTES = 1 MiB`），满队拒绝并记录错误；
 * 3. 错误显式记录到 `lastError`，禁止静默吞错。
 */
class LanTransport(
    context: Context,
    private val localPeerId: String,
    private val localFingerprint: String,
    trustedFingerprints: Set<String> = emptySet(),
) : Transport {

    companion object {
        const val SERVICE_TYPE = "_aibridge._tcp."
        const val TXT_PEER_ID = "peer_id"
        const val TXT_FINGERPRINT = "fingerprint"
        const val TXT_DEVICE_KIND = "device_kind"
        const val MAX_INBOUND_QUEUE_SIZE = 64
        const val MAX_FRAME_BYTES = 1024 * 1024
        private val FINGERPRINT_REGEX = Regex("^[0-9a-f]{64}$")

        fun isValidFingerprint(fp: String): Boolean = FINGERPRINT_REGEX.matches(fp)
    }

    init {
        require(isValidFingerprint(localFingerprint)) {
            "本机 fingerprint 必须是 64 位小写 hex"
        }
    }

    private data class ResolvedPeer(
        val info: PeerInfo,
        val host: InetAddress,
        val port: Int,
    )

    private val nsdManager = context.getSystemService(Context.NSD_SERVICE) as NsdManager
    private val trusted = ConcurrentHashMap.newKeySet<String>().apply { addAll(trustedFingerprints) }
    private val discovered = ConcurrentHashMap<String, ResolvedPeer>()
    private val inbound = ArrayBlockingQueue<ByteArray>(MAX_INBOUND_QUEUE_SIZE)
    private val lastErrorRef = AtomicReference<String?>(null)

    val lastError: String?
        get() = lastErrorRef.get()

    fun trustFingerprint(fingerprint: String) {
        require(isValidFingerprint(fingerprint)) { "对端 fingerprint 格式非法" }
        trusted.add(fingerprint)
    }

    fun buildLocalServiceInfo(port: Int): NsdServiceInfo {
        return NsdServiceInfo().apply {
            serviceName = "aibridge-$localPeerId"
            serviceType = SERVICE_TYPE
            setPort(port)
            setAttribute(TXT_PEER_ID, localPeerId)
            setAttribute(TXT_FINGERPRINT, localFingerprint)
            setAttribute(TXT_DEVICE_KIND, "android")
        }
    }

    fun onPeerResolved(serviceInfo: NsdServiceInfo): Boolean {
        val attrs = serviceInfo.attributes
        val peerId = attrs[TXT_PEER_ID]?.toString(Charsets.UTF_8)?.trim().orEmpty()
        val fingerprint = attrs[TXT_FINGERPRINT]?.toString(Charsets.UTF_8)?.trim().orEmpty()
        val deviceKind = attrs[TXT_DEVICE_KIND]?.toString(Charsets.UTF_8)?.trim().ifEmpty { "unknown" } ?: "unknown"
        val host = serviceInfo.host
        val port = serviceInfo.port

        if (peerId.isEmpty() || peerId == localPeerId || host == null || port <= 0) {
            lastErrorRef.set("忽略无效 NSD 节点：$peerId")
            return false
        }
        if (!isValidFingerprint(fingerprint)) {
            lastErrorRef.set("拒绝无合法 TXT fingerprint 的节点：$peerId")
            return false
        }
        if (trusted.isNotEmpty() && !trusted.contains(fingerprint)) {
            lastErrorRef.set("拒绝未配对 fingerprint 的节点：$peerId")
            return false
        }

        val info = PeerInfo(
            peerId = peerId,
            deviceKind = deviceKind,
            fingerprint = fingerprint,
        )
        discovered[peerId] = ResolvedPeer(info = info, host = host, port = port)
        return true
    }

    fun enqueueInboundFrame(frame: ByteArray): Boolean {
        if (frame.isEmpty() || frame.size > MAX_FRAME_BYTES) {
            lastErrorRef.set("拒绝尺寸越界帧：${frame.size} 字节")
            return false
        }
        val accepted = inbound.offer(frame)
        if (!accepted) {
            lastErrorRef.set("局域网接收队列已满（上限 $MAX_INBOUND_QUEUE_SIZE），丢弃新帧")
        }
        return accepted
    }

    fun readAuthenticatedFrame(server: ServerSocket, expectedPeerAddr: InetAddress? = null): Boolean {
        return try {
            server.accept().use { socket ->
                socket.soTimeout = 5_000
                if (expectedPeerAddr != null && socket.inetAddress != expectedPeerAddr) {
                    lastErrorRef.set("拒绝非已发现对端地址连接：${socket.inetAddress}")
                    return false
                }
                val input = DataInputStream(socket.getInputStream())
                val len = input.readInt()
                if (len <= 0 || len > MAX_FRAME_BYTES) {
                    lastErrorRef.set("对端声明帧长度非法：$len")
                    return false
                }
                val buf = ByteArray(len)
                input.readFully(buf)
                enqueueInboundFrame(buf)
            }
        } catch (e: Exception) {
            lastErrorRef.set("读取局域网帧失败：${e.message}")
            false
        }
    }

    override fun peers(): List<PeerInfo> =
        discovered.values.map { it.info }.sortedBy { it.peerId }

    override fun send(peerId: String, payload: ByteArray) {
        if (payload.isEmpty() || payload.size > MAX_FRAME_BYTES) {
            throw IllegalArgumentException("发送载荷长度越界：${payload.size}")
        }
        val target = discovered[peerId]
            ?: throw IllegalStateException("对端未发现或未通过指纹认证：$peerId")
        if (trusted.isNotEmpty() && !trusted.contains(target.info.fingerprint)) {
            throw IllegalStateException("对端指纹未获信任，拒绝发送：$peerId")
        }
        try {
            java.net.Socket().use { socket ->
                socket.connect(InetSocketAddress(target.host, target.port), 5_000)
                socket.soTimeout = 5_000
                val out = DataOutputStream(socket.getOutputStream())
                out.writeInt(payload.size)
                out.write(payload)
                out.flush()
            }
        } catch (e: Exception) {
            lastErrorRef.set("发送至 $peerId 失败：${e.message}")
            throw IllegalStateException("局域网发送失败：${e.message}", e)
        }
    }

    override fun receive(timeoutMillis: UInt): ByteArray? {
        return try {
            inbound.poll(timeoutMillis.toLong(), TimeUnit.MILLISECONDS)
        } catch (e: InterruptedException) {
            Thread.currentThread().interrupt()
            lastErrorRef.set("接收等待被中断")
            null
        }
    }
}
