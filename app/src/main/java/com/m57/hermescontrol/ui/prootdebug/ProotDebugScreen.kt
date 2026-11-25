package com.m57.hermescontrol.ui.prootdebug

import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.padding
import androidx.compose.material3.Button
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.unit.dp
import com.m57.hermescontrol.distro.ProotRuntime

@Composable
fun ProotDebugScreen(modifier: Modifier = Modifier) {
    val context = LocalContext.current
    val rt = remember { ProotRuntime(context) }
    var output by remember { mutableStateOf("") }
    Column(modifier = modifier.padding(16.dp)) {
        Text("Proot available: ${rt.isAvailable()}")
        Button(onClick = {
            Thread {
                try {
                    val inv = rt.buildCommand("uname -a")
                    val pb = ProcessBuilder(listOf(inv.program) + inv.args)
                    pb.environment().putAll(inv.env)
                    val p = pb.start()
                    output = p.inputStream.bufferedReader().readText()
                    p.waitFor()
                } catch (e: Exception) {
                    output = "Error: ${e.message}"
                }
            }.start()
        }) { Text("Run uname -a") }
        Text(output)
    }
}
