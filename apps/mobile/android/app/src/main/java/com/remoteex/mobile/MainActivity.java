package com.remoteex.mobile;

import android.app.Activity;
import android.graphics.Color;
import android.net.Uri;
import android.net.http.SslError;
import android.os.Bundle;
import android.view.ViewGroup;
import android.view.WindowManager;
import android.webkit.PermissionRequest;
import android.webkit.RenderProcessGoneDetail;
import android.webkit.SslErrorHandler;
import android.webkit.WebChromeClient;
import android.webkit.WebResourceRequest;
import android.webkit.WebResourceResponse;
import android.webkit.WebSettings;
import android.webkit.WebView;
import android.webkit.WebViewClient;
import android.widget.FrameLayout;
import android.widget.TextView;
import org.json.JSONObject;
import java.io.ByteArrayInputStream;
import java.io.ByteArrayOutputStream;
import java.io.InputStream;
import java.nio.charset.StandardCharsets;
import java.util.Arrays;
import java.util.Collections;
import java.util.HashMap;
import java.util.HashSet;
import java.util.Map;
import java.util.Set;

// Outgoing-only controller: bundled assets under a fixed secure origin.
// No JavaScript bridge, external navigation, certificate bypass, capture,
// input injection on the phone, background services or implicit sessions.
public final class MainActivity extends Activity {
    private static final String HOST = "appassets.androidplatform.net";
    private static final String HOME = "https://" + HOST + "/mobile.html";
    private static Set<String> routes(String... values) {
        return Collections.unmodifiableSet(new HashSet<>(Arrays.asList(values)));
    }
    private static final Set<String> PAGES = routes("/", "/index.html", "/mobile.html", "/identity.html", "/view.html");
    private static final Set<String> ASSETS = routes("/mobile.js", "/identity.js", "/identity-page.js", "/view-page.js", "/view-session.js", "/crypto.js", "/ice-policy.js", "/input.js", "/touch.js", "/session-policy.js", "/connection-book.js", "/wasm-loader.js", "/capabilities.js", "/controller-ui.js", "/style.css", "/pkg/noise.js", "/pkg/noise_bg.wasm", "/view-config.json");
    private FrameLayout root;
    private WebView web;
    private Map<String,String> headers;
    @Override public void onCreate(Bundle savedInstanceState) {
        super.onCreate(savedInstanceState);
        getWindow().addFlags(WindowManager.LayoutParams.FLAG_SECURE);
        root = new FrameLayout(this);
        root.setBackgroundColor(Color.WHITE);
        root.setOnApplyWindowInsetsListener((v,insets) -> {
            v.setPadding(insets.getSystemWindowInsetLeft(),insets.getSystemWindowInsetTop(),insets.getSystemWindowInsetRight(),insets.getSystemWindowInsetBottom());
            return insets.consumeSystemWindowInsets();
        });
        setContentView(root);
    }
    private void error(String text) {
        root.removeAllViews();
        TextView label=new TextView(this);label.setText(text);label.setPadding(24,24,24,24);root.addView(label);
    }
    private Map<String,String> loadHeaders() throws Exception {
        try(InputStream stream=getAssets().open("headers.json")){
            ByteArrayOutputStream buffer=new ByteArrayOutputStream();
            byte[] chunk=new byte[1024];int n;
            while((n=stream.read(chunk))!=-1){
                if(buffer.size()+n>8192)throw new IllegalStateException("Invalid asset header size");
                buffer.write(chunk,0,n);
            }
            JSONObject json=new JSONObject(new String(buffer.toByteArray(),StandardCharsets.UTF_8));
            Map<String,String> result=new HashMap<>();
            for(java.util.Iterator<String> it=json.keys();it.hasNext();){
                String name=it.next(),value=json.getString(name);
                if(value.contains("\r")||value.contains("\n"))throw new IllegalStateException("Invalid response header");
                result.put(name,value);
            }
            if(!result.containsKey("Content-Security-Policy"))throw new IllegalStateException("No CSP");
            return Collections.unmodifiableMap(result);
        }
    }
    private static boolean local(Uri u){
        return "https".equals(u.getScheme())&&HOST.equals(u.getHost())&&u.getPort()==-1&&u.getUserInfo()==null&&u.getEncodedQuery()==null&&u.getEncodedFragment()==null;
    }
    private static String route(Uri u){return local(u)?u.getEncodedPath():null;}
    private WebResourceResponse response(String type,int code,String reason,InputStream data){
        return new WebResourceResponse(type,type.equals("application/wasm")?null:"UTF-8",code,reason,headers,data);
    }
    private WebResourceResponse denied(){return response("text/plain",403,"Forbidden",new ByteArrayInputStream("Blocked".getBytes(StandardCharsets.UTF_8)));}
    private WebResourceResponse asset(WebResourceRequest req){
        String p=route(req.getUrl());
        if(p==null||!"GET".equals(req.getMethod())||(!PAGES.contains(p)&&!ASSETS.contains(p)))return denied();
        String name=p.equals("/")?"index.html":p.substring(1);
        String type=name.endsWith(".html")?"text/html":name.endsWith(".js")?"application/javascript":name.endsWith(".css")?"text/css":name.endsWith(".wasm")?"application/wasm":"application/json";
        try{return response(type,200,"OK",getAssets().open(name));}catch(Exception e){return denied();}
    }
    private void start(){
        if(web!=null)return;
        try{headers=loadHeaders();}catch(Exception e){error("Bundled client configuration unavailable.");return;}
        final WebView w=new WebView(this);
        web=w;
        WebView.setWebContentsDebuggingEnabled(false);
        WebSettings settings=w.getSettings();
        settings.setJavaScriptEnabled(true);
        settings.setDomStorageEnabled(true);
        settings.setAllowFileAccess(false);
        settings.setAllowContentAccess(false);
        settings.setAllowFileAccessFromFileURLs(false);
        settings.setAllowUniversalAccessFromFileURLs(false);
        settings.setMixedContentMode(WebSettings.MIXED_CONTENT_NEVER_ALLOW);
        settings.setSupportMultipleWindows(false);
        settings.setJavaScriptCanOpenWindowsAutomatically(false);
        settings.setSafeBrowsingEnabled(true);
        settings.setMediaPlaybackRequiresUserGesture(false);
        settings.setGeolocationEnabled(false);
        w.setWebChromeClient(new WebChromeClient(){
            @Override public void onPermissionRequest(PermissionRequest request){request.deny();}
        });
        w.setWebViewClient(new WebViewClient(){
            @Override public boolean shouldOverrideUrlLoading(WebView view,WebResourceRequest request){
                String p=route(request.getUrl());return !request.isForMainFrame()||p==null||!PAGES.contains(p);
            }
            @Override public WebResourceResponse shouldInterceptRequest(WebView view,WebResourceRequest request){return asset(request);}
            @Override public void onReceivedSslError(WebView view,SslErrorHandler handler,SslError error){handler.cancel();}
            @Override public boolean onRenderProcessGone(WebView view,RenderProcessGoneDetail detail){
                if(web==view){destroyClient();error("WebView stopped. Reopen the application to start a fresh session.");}
                return true;
            }
        });
        w.setDownloadListener((url,agent,disposition,mime,size)->{});
        root.removeAllViews();
        root.addView(w,new FrameLayout.LayoutParams(ViewGroup.LayoutParams.MATCH_PARENT,ViewGroup.LayoutParams.MATCH_PARENT));
        w.loadUrl(HOME);
    }
    private void destroyClient(){
        WebView w=web;web=null;
        if(w==null)return;
        root.removeView(w);w.stopLoading();w.onPause();w.destroy();
        // Origin-local IndexedDB remains. No session is automatically restored.
    }
    @Override protected void onResume(){super.onResume();start();}
    @Override protected void onPause(){destroyClient();super.onPause();}
    @Override protected void onDestroy(){destroyClient();super.onDestroy();}
    @Override public void onBackPressed(){
        if(web!=null&&!HOME.equals(web.getUrl())){destroyClient();start();return;}
        super.onBackPressed();
    }
}
