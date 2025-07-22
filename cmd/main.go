package main

import (
	"context"
	"log"
	"math/rand/v2"
	"net/http"
	"net/http/httputil"
	"net/url"
	"strings"

	"github.com/docker/docker/api/types/container"
	"github.com/docker/docker/client"
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
					Servers:       []string{"http://172.18.0.2:80"},
				},
				"host2": Subdomain{
					SubdomainPath: "host2",
					Servers:       []string{"http://host2"},
				},
				"host3": Subdomain{
					SubdomainPath: "host2",
					Servers:       []string{"http://host3"},
				},
				"rep": Subdomain{
					SubdomainPath: "rep",
					Servers:       []string{"http://host-replicated"},
				},
				"random": Subdomain{
					SubdomainPath: "random",
					Servers: []string{
						"http://host1",
						"http://host2",
						"http://host3",
					},
				},
			},
		},
	}

	cli, err := client.NewClientWithOpts(client.FromEnv, client.WithAPIVersionNegotiation())
	if err != nil {
		log.Print("Error initializing Docker client: ")
		log.Fatal(err)
	}
	defer cli.Close()

	containers, err := cli.ContainerList(context.Background(), container.ListOptions{All: true})
	if err != nil {
		panic(err)
	}

	for _, ctr := range containers {
		log.Printf("%s %s (status: %s)\n", ctr.ID, ctr.Image, ctr.Status)
	}

	// Start the HTTP server
	port := "8080" // Replace with your desired port
	log.Printf("Proxy server listening on port %s", port)
	log.Fatal(http.ListenAndServe(":"+port, &ProxyHandler{}))
}
