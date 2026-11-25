package com.m57.hermescontrol.distro

import android.content.Context
import android.content.SharedPreferences
import java.io.File
import java.util.UUID

object DistroConfig {
    private const val PREFS = "distro_config"
    private const val KEY_TOKEN = "hermesd_token"

    // Debian arm64 mini rootfs (placeholder; replace with a pinned, checksummed tarball)
    const val ROOTFS_URL = ""
    const val ROOTFS_SHA256 = ""

    fun token(context: Context): String {
        val prefs: SharedPreferences = context.getSharedPreferences(PREFS, Context.MODE_PRIVATE)
        var t = prefs.getString(KEY_TOKEN, null)
        if (t.isNullOrBlank()) {
            t = UUID.randomUUID().toString().replace("-", "")
            prefs.edit().putString(KEY_TOKEN, t).apply()
        }
        return t!!
    }

    fun baseDir(context: Context): File = File(context.filesDir, "distro").also { it.mkdirs() }

    fun manifestsDir(context: Context): File = File(baseDir(context), "stacks").also { it.mkdirs() }

    fun logsFile(context: Context): File = File(baseDir(context), "hermesd.log")

    fun appendLog(context: Context, line: String) {
        try {
            logsFile(context).appendText(line + "\n")
        } catch (_: Exception) {
        }
    }

    fun ensureDefaultManifests(dir: File) {
        val hermes = File(dir, "hermes.toml")
        if (!hermes.exists()) {
            hermes.writeText(
                """
                [stack]
                name = "hermes"
                autostart = true

                [[service]]
                name = "dashboard"
                cmd = "/opt/hermes/venv/bin/hermes dashboard --host 127.0.0.1 --port 9000"
                cwd = "/root"
                restart = "on-failure"
                max_rss_mb = 900
                env = { HERMES_HOME = "/root/.hermes" }
                """.trimIndent(),
            )
        }
        val mcps = File(dir, "mcps.toml")
        if (!mcps.exists()) {
            mcps.writeText(
                """
                [stack]
                name = "mcps"
                autostart = false

                [[service]]
                name = "example"
                cmd = "echo mcp placeholder"
                restart = "no"
                """.trimIndent(),
            )
        }
        val outros = File(dir, "outros.toml")
        if (!outros.exists()) {
            outros.writeText(
                """
                [stack]
                name = "outros"
                autostart = false
                """.trimIndent(),
            )
        }
    }
}
