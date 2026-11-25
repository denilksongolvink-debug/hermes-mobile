package com.m57.hermescontrol.distro

import android.content.Context
import java.io.File

data class ProotInvocation(
    val program: String,
    val args: List<String>,
    val env: Map<String, String>,
)

class ProotRuntime(
    private val context: Context,
) {
    val prootBinary: File? by lazy {
        val dir = context.applicationInfo.nativeLibraryDir
        val f = File(dir, "libproot-xed.so")
        if (f.exists()) f else null
    }

    val rootfsDir: File
        get() = File(context.filesDir, "rootfs")

    val hermesdBinary: File? by lazy {
        val dir = context.applicationInfo.nativeLibraryDir
        val f = File(dir, "libhermesd.so")
        if (f.exists()) f else null
    }

    fun isAvailable(): Boolean = prootBinary != null

    fun buildCommand(
        innerCmd: String,
        cwd: String = "/",
    ): ProotInvocation {
        val bin = hermesdBinary?.absolutePath
        if (bin != null) {
            try {
                val pb = ProcessBuilder(
                    bin,
                    "--print-proot-cmd",
                    "--native-dir",
                    context.applicationInfo.nativeLibraryDir,
                    "--files-dir",
                    context.filesDir.absolutePath,
                    "--rootfs",
                    rootfsDir.absolutePath,
                    "--inner-cmd",
                    innerCmd,
                    "--cwd",
                    cwd,
                )
                val p = pb.start()
                val json = p.inputStream.bufferedReader().readText()
                p.waitFor()
                val obj = org.json.JSONObject(json)
                val arr = obj.getJSONArray("args")
                val env = obj.getJSONObject("env")
                val cmd = mutableListOf<String>()
                for (i in 0 until arr.length()) cmd.add(arr.getString(i))
                val envMap = mutableMapOf<String, String>()
                val eit = env.keys()
                while (eit.hasNext()) {
                    val k = eit.next()
                    envMap[k] = env.getString(k)
                }
                return ProotInvocation(obj.getString("program"), cmd, envMap)
            } catch (_: Exception) {
            }
        }
        return ProotInvocation("proot", emptyList(), emptyMap())
    }

    fun hermesdCommand(
        host: String = "127.0.0.1",
        port: Int = 9876,
        token: String,
        manifestsDir: File,
        rootfsUrl: String = DistroConfig.ROOTFS_URL,
        rootfsSha256: String = DistroConfig.ROOTFS_SHA256,
    ): ProotInvocation {
        val bin = hermesdBinary?.absolutePath ?: "libhermesd.so"
        val cmd = mutableListOf(
            bin,
            "--host",
            host,
            "--port",
            port.toString(),
            "--token",
            token,
            "--native-dir",
            context.applicationInfo.nativeLibraryDir,
            "--files-dir",
            context.filesDir.absolutePath,
            "--rootfs",
            rootfsDir.absolutePath,
            "--manifests",
            manifestsDir.absolutePath,
        )
        if (rootfsUrl.isNotBlank()) {
            cmd += listOf("--rootfs-url", rootfsUrl)
        }
        if (rootfsSha256.isNotBlank()) {
            cmd += listOf("--rootfs-sha256", rootfsSha256)
        }
        return cmd
    }
}
