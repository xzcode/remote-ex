package com.remoteex.mobile;

import android.app.Activity;
import android.app.Instrumentation;
import android.content.Intent;
import android.content.pm.PackageInfo;
import android.os.Bundle;
import android.os.SystemClock;
import android.view.View;
import android.view.ViewGroup;
import android.webkit.WebView;
import java.util.concurrent.Callable;
import java.util.concurrent.CountDownLatch;
import java.util.concurrent.TimeUnit;
import java.util.concurrent.atomic.AtomicReference;
import org.json.JSONArray;

// Separate test APK. Fixed offline probes only, no remote host or OS injection.
public final class OfflineSmoke extends Instrumentation {
    private static final String HOME = "https://appassets.androidplatform.net/mobile.html";
    private static final String IDENTITY = "https://appassets.androidplatform.net/identity.html";
    @Override public void onCreate(Bundle args) {
        super.onCreate(args);
        start();
    }
    private static void require(boolean ok, String message) {
        if (!ok) throw new AssertionError(message);
    }
    private <T> T main(Callable<T> work) throws Exception {
        AtomicReference<T> value = new AtomicReference<>();
        AtomicReference<Throwable> failure = new AtomicReference<>();
        runOnMainSync(() -> {
            try { value.set(work.call()); }
            catch (Throwable e) { failure.set(e); }
        });
        if (failure.get() != null) throw new IllegalStateException("UI-thread probe failed", failure.get());
        return value.get();
    }
    private static WebView findWeb(View v) {
        if (v instanceof WebView) return (WebView) v;
        if (v instanceof ViewGroup) {
            ViewGroup parent = (ViewGroup) v;
            for (int i = 0; i < parent.getChildCount(); i++) {
                WebView child = findWeb(parent.getChildAt(i));
                if (child != null) return child;
            }
        }
        return null;
    }
    private WebView web(Activity a) throws Exception {
        return main(() -> findWeb(a.findViewById(android.R.id.content)));
    }
    private Activity launch() {
        Intent i = new Intent(Intent.ACTION_MAIN);
        i.setClassName(getTargetContext().getPackageName(), "com.remoteex.mobile.MainActivity");
        i.addFlags(Intent.FLAG_ACTIVITY_NEW_TASK | Intent.FLAG_ACTIVITY_CLEAR_TASK);
        return startActivitySync(i);
    }
    private void until(Callable<Boolean> condition, int seconds, String message) throws Exception {
        long end = SystemClock.elapsedRealtime() + seconds * 1000L;
        while (SystemClock.elapsedRealtime() < end) {
            if (Boolean.TRUE.equals(condition.call())) return;
            SystemClock.sleep(150);
        }
        throw new AssertionError(message);
    }
    private String js(WebView w, String source) throws Exception {
        CountDownLatch done = new CountDownLatch(1);
        AtomicReference<String> raw = new AtomicReference<>();
        main(() -> {
            w.evaluateJavascript(source, value -> { raw.set(value); done.countDown(); });
            return null;
        });
        require(done.await(3, TimeUnit.SECONDS), "WebView probe callback timed out");
        return raw.get();
    }
    private String string(WebView w, String source) throws Exception {
        return new JSONArray("[" + js(w, source) + "]").optString(0, "");
    }
    private boolean yes(WebView w, String source) throws Exception {
        return "true".equals(js(w, source));
    }
    private WebView landing(Activity a) throws Exception {
        until(() -> {
            WebView w = web(a);
            return w != null && HOME.equals(main(w::getUrl));
        }, 20, "Bundled landing page did not load");
        WebView w = web(a);
        until(() -> yes(w, "Boolean(document.querySelector('#mobile-policy') && document.querySelector('#mobile-policy').textContent.includes('Identity management is local only.'))"),
            40, "Offline configuration or capability page did not settle");
        require(yes(w, "isSecureContext && location.origin==='https://appassets.androidplatform.net' && document.querySelector('#mobile-connect').hidden"), "Offline origin/connect gate failed");
        require(yes(w, "document.querySelectorAll('#mobile-checks li').length===6"), "Capability report incomplete");
        String report=string(w, "Array.from(document.querySelectorAll('#mobile-checks li'),e=>e.dataset.capability+':'+e.dataset.pass+':'+e.dataset.code).join(' ')");
        require(report.matches("[A-Za-z0-9_: -]{1,600}"),"Invalid capability report shape");
        PackageInfo provider=main(WebView::getCurrentWebViewPackage);
        String version=provider==null?"unavailable":String.valueOf(provider.versionName).replaceAll("[^A-Za-z0-9._-]","_");
        if(version.length()>80)version=version.substring(0,80);
        android.util.Log.i("RemoteEXOfflineSmoke","WebView version="+version+" "+report);
        require(yes(w, "document.querySelectorAll('#mobile-checks li[data-pass=true]').length===6"), "WebView "+version+" missing capabilities: "+report);
        require(main(() -> !w.getSettings().getAllowFileAccess() &&
                            !w.getSettings().getAllowContentAccess() &&
                            !w.getSettings().getJavaScriptCanOpenWindowsAutomatically()),
            "WebView file or popup policy changed");
        return w;
    }
    private void navigate(WebView w, String url) throws Exception {
        main(() -> { w.loadUrl(url); return null; });
        until(() -> url.equals(main(w::getUrl)) &&
                    yes(w, "Boolean(document.querySelector('#identity-status'))"), 20,
            "Identity page unavailable");
        until(() -> yes(w, "['idle','verified','error'].includes(document.querySelector('#identity-status').dataset.state)"), 20,
            "Identity status did not settle");
    }
    private String identity(WebView w) throws Exception {
        until(() -> yes(w, "document.querySelector('#identity-status').dataset.state==='verified' || document.querySelector('#identity-status').dataset.state==='error'"), 20,
            "Identity operation timed out");
        require(yes(w, "document.querySelector('#identity-status').dataset.state==='verified'"),
            "Identity generation or stored key proof failed");
        String value = string(w, "document.querySelector('#device-id').value");
        require(value.matches("[0-9a-f]{64}") && !value.matches("0+"), "Invalid local public fingerprint");
        return value;
    }
    private void fullSmoke() throws Exception {
        Activity a = launch();
        String saved;
        try {
            WebView first = landing(a);
            navigate(first, IDENTITY);
            require(yes(first, "document.querySelector('#identity-status').dataset.state==='idle'"), "Expected fresh test installation");
            require(yes(first, "!document.querySelector('#identity-create').disabled"), "Identity create unavailable");
            js(first, "document.querySelector('#identity-create').click(); true");
            saved = identity(first);
            // Ordinary foreground transition, not a fabricated lifecycle callback.
            main(() -> {
                Intent home = new Intent(Intent.ACTION_MAIN);
                home.addCategory(Intent.CATEGORY_HOME);
                home.addFlags(Intent.FLAG_ACTIVITY_NEW_TASK);
                a.startActivity(home);
                return null;
            });
            until(() -> web(a) == null, 15, "Background retained the WebView");
        } finally {
            main(() -> { if (!a.isFinishing()) a.finish(); return null; });
        }
        until(() -> main(a::isDestroyed), 15, "Previous activity not destroyed");
        Activity b = launch();
        try {
            WebView second = landing(b);
            navigate(second, IDENTITY);
            require(saved.equals(identity(second)), "Private identity did not survive activity recreation");
            require(yes(second, "document.querySelector('#identity-create').disabled"), "Existing identity can be overwritten");
            js(second, "document.querySelector('#identity-confirm').click();document.querySelector('#identity-delete').click();true");
            until(() -> yes(second, "document.querySelector('#identity-status').dataset.state==='idle' && !document.querySelector('#device-id').value"), 20,
                "Explicit identity deletion did not complete");
            main(() -> { second.loadUrl(HOME); return null; });
            landing(b);
        } finally {
            main(() -> { if (!b.isFinishing()) b.finish(); return null; });
        }
    }
    @Override public void onStart() {
        Bundle result = new Bundle();
        try {
            fullSmoke();
            result.putString("stream", "\nPASS REMOTE EX ANDROID OFFLINE RUNTIME: installed WebView, secure origin, crypto/WASM capabilities, persistent identity, background destruction, fresh landing, explicit deletion\n");
            finish(Activity.RESULT_OK, result);
        } catch (Throwable e) {
            result.putString("stream", "\nFAIL REMOTE EX ANDROID OFFLINE RUNTIME: " + e.getClass().getSimpleName() + ": " + e.getMessage() + "\n");
            finish(Activity.RESULT_CANCELED, result);
        }
    }
}
