package main

import (
	"log"
	"math/rand/v2"
	"net/http"
	"net/http/httputil"
	"net/url"
	"strings"
)

var (
	Domains map[string]Domain
)

type Domain struct {
	DomainPath string
	Subdomains map[string]Subdomain
}

type Subdomain struct {
	SubdomainPath string
	Servers       []string
}

func splitHostPort(host string) (subdomain, domain string) {
	hostPortSplit := strings.Split(host, ":")

	// TODO: handle basic Auth
	if len(hostPortSplit) > 2 {
		log.Panic("Basic Auth in URL not supported")
	}

	hostWoPort := hostPortSplit[0]

	domainSplit := strings.Split(hostWoPort, ".")
	subdomain = domainSplit[0]
	domain = strings.Join(domainSplit[1:], ".")

	return subdomain, domain
}

type ProxyHandler struct{}

func (h *ProxyHandler) ServeHTTP(res http.ResponseWriter, req *http.Request) {
	var (
		target *url.URL
		err    error
	)

	log.Printf("Received request: %s %s %s", req.Method, req.URL, req.Host)
	subdomain, domain := splitHostPort(req.Host)

	if d, e := Domains[domain]; e {
		log.Println(d)
		if s, e := d.Subdomains[subdomain]; e {
			log.Println(s)
			var server string
			if len(s.Servers) == 0 {
				server = s.Servers[0]
			} else {
				server = s.Servers[rand.IntN(len(s.Servers))]
			}

			target, err = url.Parse(server)
			if err != nil {
				log.Println(err)
				http.Error(res, "Invalid target URL", http.StatusInternalServerError)
				return
			}
			log.Println("Target: ", target)
		} else {
			log.Println("No domain found")
			return
		}
	} else {
		log.Println("No domain found")
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
	Domains = map[string]Domain{
		"localhost": Domain{
			DomainPath: "localhost",
			Subdomains: map[string]Subdomain{
				"test": Subdomain{
					SubdomainPath: "test",
					Servers:       []string{"http://httpbin.org"},
				},
				"host1": Subdomain{
					SubdomainPath: "host1",
					Servers:       []string{"http://localhost:9001"},
				},
				"host2": Subdomain{
					SubdomainPath: "host2",
					Servers:       []string{"http://localhost:9002"},
				},
				"random": Subdomain{
					SubdomainPath: "random",
					Servers: []string{
						"http://httpbin.org",
						"http://cht.sh",
						"http://google.com",
					},
				},
			},
		},
	}

	// Start the HTTP server
	port := "8080" // Replace with your desired port
	log.Printf("Proxy server listening on port %s", port)
	log.Fatal(http.ListenAndServe(":"+port, &ProxyHandler{}))
}
