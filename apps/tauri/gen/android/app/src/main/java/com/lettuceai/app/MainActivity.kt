package com.lettuceai.app

import android.os.Bundle
import android.view.View
import android.view.ViewTreeObserver
import android.webkit.WebView
import androidx.activity.enableEdgeToEdge
import androidx.core.graphics.Insets
import androidx.core.view.ViewCompat
import androidx.core.view.WindowInsetsCompat
import java.util.Locale

class MainActivity : TauriActivity() {
  // The backend stays ready when Android recreates the activity in this process.
  private companion object {
    var startupFinished = false
  }

  private var webView: WebView? = null
  private var safeTopCss = 0f
  private var safeRightCss = 0f
  private var safeBottomCss = 0f
  private var safeLeftCss = 0f

  override fun onCreate(savedInstanceState: Bundle?) {
    enableEdgeToEdge()
    super.onCreate(savedInstanceState)
    val appContent = findViewById<View>(android.R.id.content)
    val initialBottomPadding = appContent.paddingBottom
    ViewCompat.setOnApplyWindowInsetsListener(appContent) { view, insets ->
      // Handle IME insets natively so the layout viewport also shrinks.
      val ime = insets.getInsets(WindowInsetsCompat.Type.ime())
      view.setPadding(view.paddingLeft, view.paddingTop, view.paddingRight, initialBottomPadding + ime.bottom)
      // Android 15+ draws edge-to-edge; WebView often reports env(safe-area-inset-*) as 0.
      // Push system bar and cutout insets into the CSS variables Crisp and the UI already use.
      // When the keyboard is up the content view is already padded, so bottom must be zero.
      val bars = insets.getInsets(
        WindowInsetsCompat.Type.systemBars() or WindowInsetsCompat.Type.displayCutout()
      )
      val density = view.resources.displayMetrics.density.let { if (it > 0f) it else 1f }
      safeTopCss = bars.top / density
      safeRightCss = bars.right / density
      safeBottomCss = if (ime.bottom > 0) 0f else bars.bottom / density
      safeLeftCss = bars.left / density
      injectSafeAreaCss()
      val remaining = WindowInsetsCompat.Builder(insets)
        .setInsets(WindowInsetsCompat.Type.ime(), Insets.NONE)
      if (ime.bottom > 0) {
        val navigation = insets.getInsets(WindowInsetsCompat.Type.navigationBars())
        remaining.setInsets(WindowInsetsCompat.Type.navigationBars(), Insets.of(navigation.left, navigation.top, navigation.right, 0))
      }
      remaining.build()
    }
    ViewCompat.requestApplyInsets(appContent)
    val content = window.decorView
    content.viewTreeObserver.addOnPreDrawListener(object : ViewTreeObserver.OnPreDrawListener {
      override fun onPreDraw(): Boolean {
        if (!startupFinished) return false
        content.viewTreeObserver.removeOnPreDrawListener(this)
        return true
      }
    })
  }

  override fun onWebViewCreate(webView: WebView) {
    this.webView = webView
    injectSafeAreaCss()
    // The first document may still be loading; retry once the view is attached and laid out.
    webView.addOnAttachStateChangeListener(object : View.OnAttachStateChangeListener {
      override fun onViewAttachedToWindow(v: View) {
        injectSafeAreaCss()
      }
      override fun onViewDetachedFromWindow(v: View) {}
    })
    webView.post { injectSafeAreaCss() }
  }

  override fun onResume() {
    super.onResume()
    injectSafeAreaCss()
  }

  private fun injectSafeAreaCss() {
    val view = webView ?: return
    val script = (
      "(function(){var r=document.documentElement;if(!r||!r.style)return;" +
        "r.style.setProperty('--safe-top','${cssPx(safeTopCss)}');" +
        "r.style.setProperty('--safe-right','${cssPx(safeRightCss)}');" +
        "r.style.setProperty('--safe-bottom','${cssPx(safeBottomCss)}');" +
        "r.style.setProperty('--safe-left','${cssPx(safeLeftCss)}');" +
        "})();"
      )
    view.post { view.evaluateJavascript(script, null) }
  }

  private fun cssPx(value: Float): String = String.format(Locale.US, "%.2fpx", value)

  fun finishStartup() {
    runOnUiThread {
      startupFinished = true
      window.decorView.invalidate()
    }
  }
}
