package main

import (
	"log"
	"net/http"
	"net/http/httputil"
	"net/url"
)

type ProxyHandler struct{}

func (h *ProxyHandler) ServeHTTP(res http.ResponseWriter, req *http.Request) {
	log.Printf("Received request: %s %s %s", req.Method, req.URL, req.Header)
	target, err := url.Parse("http://httpbin.org/") // Replace with the actual target URL
	if err != nil {
		log.Println(err)
		http.Error(res, "Invalid target URL", http.StatusInternalServerError)
		return
	}

	// Update the request URL to point to the target
	req.URL.Scheme = target.Scheme
	req.URL.Host = target.Host

	req.Header.Set("Host", target.Host)

	// Forward the request to the target server
	// proxy := httputil.NewSingleHostReverseProxy(target)
	proxy := &httputil.ReverseProxy{
		Rewrite: func(r *httputil.ProxyRequest) {
			r.SetURL(target)
			r.Out.Host = r.In.Host // if desired
		},
	}

	proxy.ServeHTTP(res, req)
}

func main() {
	// Start the HTTP server
	port := "8080" // Replace with your desired port
	log.Printf("Proxy server listening on port %s", port)
	log.Fatal(http.ListenAndServe(":"+port, &ProxyHandler{}))
}
