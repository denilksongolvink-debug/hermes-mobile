package com.m57.hermescontrol.distro

import android.content.Context
import com.m57.hermescontrol.distro.DistroConfig
import java.io.File

/**
 * Wraps the proot binary shipped in jniLibs (extracted to nativeLibraryDir thanks to
 * useLegacyPackaging). Builds command lines to run programs inside the rootfs.
 */
class ProotRuntime(private val context: Context) {
    val prootBinary: File? by lazy {
        val dir = context.applicationInfo.nativeLibraryDir
        val f = File(dir, "libproot.so")
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

    /**
     * Build a command to run [innerCmd] inside the rootfs.
     */
    fun buildCommand(innerCmd: String, cwd: String = "/"): List<String> {
        val proot = prootBinary?.absolutePath ?: "proot"
        val rootfs = rootfsDir.absolutePath
        return listOf(
            proot,
            "-r", rootfs,
            "-b", "/dev",
            "-b", "/proc",
            "-b", "/sys",
            "-w", cwd,
            "/bin/sh", "-c", innerCmd,
        )
    }

    fun hermesdCommand(
        host: String = "127.0.0.1",
        port: Int = 9876,
        token: String,
        manifestsDir: File,
        rootfsUrl: String = DistroConfig.ROOTFS_URL,
        rootfsSha256: String = DistroConfig.ROOTFS_SHA256,
    ): List<String> {
        val bin = hermesdBinary?.absolutePath ?: "libhermesd.so"
        val cmd = mutableListOf(
            bin,
            "--host", host,
            "--port", port.toString(),
            "--token", token,
            "--proot", prootBinary?.absolutePath ?: "proot",
            "--rootfs", rootfsDir.absolutePath,
            "--manifests", manifestsDir.absolutePath,
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
