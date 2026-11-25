package com.m57.hermescontrol.ui.stacks

import android.util.Log
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.material3.Button
import androidx.compose.material3.Card
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.unit.dp
import com.m57.hermescontrol.distro.DistroConfig
import com.m57.hermescontrol.distro.DistroService
import org.json.JSONArray
import org.json.JSONObject
import java.net.HttpURLConnection
import java.net.URL

data class ServiceView(
    val stack: String,
    val name: String,
    val status: String,
    val pid: Int?,
    val rssKb: Long?,
    val restarts: Int,
)

data class StackView(val name: String, val services: List<ServiceView>)

@Composable
fun StacksScreen(modifier: Modifier = Modifier) {
    val context = LocalContext.current
    val token = remember { DistroConfig.token(context) }
    var stacks by remember { mutableStateOf<List<StackView>>(emptyList()) }
    var error by remember { mutableStateOf<String?>(null) }

    fun refresh() {
        Thread {
            try {
                val url = URL("http://127.0.0.1:9876/v1/stacks")
                val conn = url.openConnection() as HttpURLConnection
                conn.setRequestProperty("Authorization", "Bearer $token")
                val text = conn.inputStream.bufferedReader().readText()
                val arr = JSONArray(text)
                val list = mutableListOf<StackView>()
                for (i in 0 until arr.length()) {
                    val s = arr.getJSONObject(i)
                    val servicesArr = s.getJSONArray("services")
                    val svcs = mutableListOf<ServiceView>()
                    for (j in 0 until servicesArr.length()) {
                        val svc = servicesArr.getJSONObject(j)
                        svcs += ServiceView(
                            stack = s.getString("name"),
                            name = svc.getString("name"),
                            status = svc.optString("status"),
                            pid = if (svc.has("pid") && !svc.isNull("pid")) svc.getInt("pid") else null,
                            rssKb = if (svc.has("rss_kb") && !svc.isNull("rss_kb")) svc.getLong("rss_kb") else null,
                            restarts = svc.optInt("restarts"),
                        )
                    }
                    list += StackView(s.getString("name"), svcs)
                }
                stacks = list
                error = null
            } catch (e: Exception) {
                error = e.message
            }
        }.start()
    }

    LaunchedEffect(Unit) {
        DistroService.start(context)
        refresh()
    }

    Column(modifier = modifier.fillMaxSize().padding(16.dp)) {
        Row(Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.SpaceBetween) {
            Text("Stacks", style = MaterialTheme.typography.headlineSmall)
            Button(onClick = { refresh() }) { Text("Refresh") }
        }
        error?.let { Text("Error: $it", color = MaterialTheme.colorScheme.error) }
        LazyColumn(verticalArrangement = Arrangement.spacedBy(12.dp)) {
            items(stacks) { stack ->
                Card(Modifier.fillMaxWidth()) {
                    Column(Modifier.padding(12.dp)) {
                        Text(stack.name, style = MaterialTheme.typography.titleMedium)
                        stack.services.forEach { svc ->
                            Row(Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.SpaceBetween) {
                                Column {
                                    Text("${svc.name}: ${svc.status}")
                                    Text("RSS: ${svc.rssKb ?: 0} KB, restarts: ${svc.restarts}", style = MaterialTheme.typography.bodySmall)
                                }
                                Row(horizontalArrangement = Arrangement.spacedBy(4.dp)) {
                                    Button(onClick = { postAction(token, svc.stack, svc.name, "start"); refresh() }) { Text("Start") }
                                    Button(onClick = { postAction(token, svc.stack, svc.name, "stop"); refresh() }) { Text("Stop") }
                                    Button(onClick = { postAction(token, svc.stack, svc.name, "restart"); refresh() }) { Text("Restart") }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

private fun postAction(token: String, stack: String, service: String, action: String) {
    Thread {
        try {
            val url = URL("http://127.0.0.1:9876/v1/stacks/$stack/services/$service/$action")
            val conn = url.openConnection() as HttpURLConnection
            conn.requestMethod = "POST"
            conn.setRequestProperty("Authorization", "Bearer $token")
            conn.responseCode
            conn.disconnect()
        } catch (e: Exception) {
            Log.e("Stacks", "action failed", e)
        }
    }.start()
}
