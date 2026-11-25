package com.m57.hermescontrol.distro

import android.app.Notification
import android.app.NotificationChannel
import android.app.NotificationManager
import android.app.PendingIntent
import android.app.Service
import android.content.Context
import android.content.Intent
import android.content.pm.ServiceInfo
import android.os.Build
import android.os.IBinder
import android.os.PowerManager
import androidx.core.app.NotificationCompat
import com.m57.hermescontrol.R
import java.io.File

/**
 * Foreground service that launches and supervises the embedded distro runtime (hermesd).
 * Keeps the process alive in background with an optional partial wake lock.
 */
class DistroService : Service() {
    private var process: Process? = null
    private var wakeLock: PowerManager.WakeLock? = null
    private var wakeLockHeld: Boolean = false

    override fun onBind(intent: Intent?): IBinder? = null

    override fun onCreate() {
        super.onCreate()
        createChannel()
        startForegroundWithNotification()
    }

    override fun onStartCommand(intent: Intent?, flags: Int, startId: Int): Int {
        when (intent?.action) {
            ACTION_TOGGLE_WAKE_LOCK -> toggleWakeLock()
            ACTION_STOP -> {
                stopSelf()
                return START_NOT_STICKY
            }
            else -> {
                if (process == null) launch()
            }
        }
        return START_STICKY
    }

    private fun launch() {
        val rt = ProotRuntime(this)
        val token = DistroConfig.token(this)
        val manifestsDir = DistroConfig.manifestsDir(this)
        manifestsDir.mkdirs()
        // ensure default manifests exist
        DistroConfig.ensureDefaultManifests(manifestsDir)
        val cmd = rt.hermesdCommand(token = token, manifestsDir = manifestsDir, rootfsUrl = DistroConfig.ROOTFS_URL, rootfsSha256 = DistroConfig.ROOTFS_SHA256)
        try {
            val pb = ProcessBuilder(cmd)
            pb.redirectErrorStream(true)
            pb.directory(filesDir)
            process = pb.start()
            // drain output to a log file
            Thread {
                try {
                    process?.inputStream?.bufferedReader()?.forEachLine { line ->
                        DistroConfig.appendLog(this, line)
                    }
                } catch (_: Exception) {
                }
            }.start()
        } catch (e: Exception) {
            DistroConfig.appendLog(this, "Failed to launch hermesd: ${e.message}")
        }
    }

    override fun onDestroy() {
        process?.destroy()
        process = null
        releaseWakeLock()
        super.onDestroy()
    }

    private fun createChannel() {
        val mgr = getSystemService(NotificationManager::class.java)
        mgr.createNotificationChannel(
            NotificationChannel(CHANNEL_ID, "Distro", NotificationManager.IMPORTANCE_LOW),
        )
    }

    private fun notification(): Notification {
        val toggleIntent = Intent(this, DistroService::class.java).setAction(ACTION_TOGGLE_WAKE_LOCK)
        val togglePi = PendingIntent.getService(this, 1, toggleIntent, PendingIntent.FLAG_IMMUTABLE)
        val stopIntent = Intent(this, DistroService::class.java).setAction(ACTION_STOP)
        val stopPi = PendingIntent.getService(this, 2, stopIntent, PendingIntent.FLAG_IMMUTABLE)
        return NotificationCompat.Builder(this, CHANNEL_ID)
            .setSmallIcon(R.drawable.ic_notification)
            .setContentTitle("Hermes embedded")
            .setContentText(if (wakeLockHeld) "Running (wake lock on)" else "Running")
            .setOngoing(true)
            .addAction(0, "Toggle wake lock", togglePi)
            .addAction(0, "Stop", stopPi)
            .build()
    }

    private fun startForegroundWithNotification() {
        val n = notification()
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.UPSIDE_DOWN_CAKE) {
            startForeground(
                NOTIFICATION_ID,
                n,
                ServiceInfo.FOREGROUND_SERVICE_TYPE_SPECIAL_USE,
            )
        } else {
            startForeground(NOTIFICATION_ID, n)
        }
    }

    private fun toggleWakeLock() {
        val pm = getSystemService(Context.POWER_SERVICE) as PowerManager
        if (!wakeLockHeld) {
            if (wakeLock == null) {
                wakeLock = pm.newWakeLock(PowerManager.PARTIAL_WAKE_LOCK, "HermesDistro:WakeLock")
            }
            wakeLock?.acquire()
            wakeLockHeld = true
        } else {
            wakeLock?.release()
            wakeLockHeld = false
        }
        val mgr = getSystemService(NotificationManager::class.java)
        mgr.notify(NOTIFICATION_ID, notification())
    }

    private fun releaseWakeLock() {
        if (wakeLockHeld) {
            wakeLock?.release()
            wakeLockHeld = false
        }
    }

    companion object {
        private const val CHANNEL_ID = "distro_service"
        private const val NOTIFICATION_ID = 0x4D15
        const val ACTION_TOGGLE_WAKE_LOCK = "com.m57.hermescontrol.distro.TOGGLE_WAKE_LOCK"
        const val ACTION_STOP = "com.m57.hermescontrol.distro.STOP"

        fun start(context: Context) {
            val appContext = context.applicationContext
            val intent = Intent(appContext, DistroService::class.java)
            if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.O) {
                appContext.startForegroundService(intent)
            } else {
                appContext.startService(intent)
            }
        }

        fun stop(context: Context) {
            val appContext = context.applicationContext
            appContext.stopService(Intent(appContext, DistroService::class.java))
        }
    }
}
