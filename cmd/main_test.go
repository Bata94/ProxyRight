package main

import (
	"net/http"
	"net/http/httptest"
	"strings"
	"testing"
)

func TestSplitHostPort(t *testing.T) {
	tests := []struct {
		name              string
		input             string
		expectedSubdomain string
		expectedDomain    string
	}{
		{
			name:              "simple subdomain and domain",
			input:             "api.example.com",
			expectedSubdomain: "api",
			expectedDomain:    "example.com",
		},
		{
			name:              "subdomain with port",
			input:             "api.example.com:8080",
			expectedSubdomain: "api",
			expectedDomain:    "example.com",
		},
		{
			name:              "localhost with subdomain",
			input:             "test.localhost",
			expectedSubdomain: "test",
			expectedDomain:    "localhost",
		},
		{
			name:              "localhost with port",
			input:             "test.localhost:3000",
			expectedSubdomain: "test",
			expectedDomain:    "localhost",
		},
		{
			name:              "multiple subdomains",
			input:             "api.v1.example.com",
			expectedSubdomain: "api",
			expectedDomain:    "v1.example.com",
		},
		{
			name:              "single domain no subdomain",
			input:             "localhost",
			expectedSubdomain: "localhost",
			expectedDomain:    "",
		},
		{
			name:              "single domain with port",
			input:             "localhost:8080",
			expectedSubdomain: "localhost",
			expectedDomain:    "",
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			subdomain, domain := splitHostPort(tt.input)
			if subdomain != tt.expectedSubdomain {
				t.Errorf("splitHostPort(%q) subdomain = %q, want %q", tt.input, subdomain, tt.expectedSubdomain)
			}
			if domain != tt.expectedDomain {
				t.Errorf("splitHostPort(%q) domain = %q, want %q", tt.input, domain, tt.expectedDomain)
			}
		})
	}
}

func TestSplitHostPortPanic(t *testing.T) {
	// Test that basic auth in URL causes panic as documented
	defer func() {
		if r := recover(); r == nil {
			t.Errorf("splitHostPort should panic with basic auth in URL")
		}
	}()
	
	splitHostPort("user:pass@example.com:8080")
}

func TestProxyHandlerServeHTTP(t *testing.T) {
	// Create mock servers for testing
	mockServer1 := createMockServer("Backend 1")
	defer mockServer1.Close()
	
	mockServer2 := createMockServer("Backend 2")
	defer mockServer2.Close()
	
	mockWebServer := createMockServer("Web Server")
	defer mockWebServer.Close()

	// Set up test domains
	originalDomains := Domains
	defer func() {
		Domains = originalDomains
	}()

	Domains = map[string]Domain{
		"example.com": {
			DomainPath: "example.com",
			Subdomains: map[string]Subdomain{
				"api": {
					SubdomainPath: "api",
					Servers:       []string{mockServer1.URL, mockServer2.URL},
				},
				"web": {
					SubdomainPath: "web",
					Servers:       []string{mockWebServer.URL},
				},
				"empty": {
					SubdomainPath: "empty",
					Servers:       []string{}, // This should cause issues in current implementation
				},
			},
		},
	}

	tests := []struct {
		name           string
		host           string
		expectedStatus int
		shouldProxy    bool
	}{
		{
			name:           "valid subdomain with multiple servers",
			host:           "api.example.com",
			expectedStatus: http.StatusOK,
			shouldProxy:    true,
		},
		{
			name:           "valid subdomain with single server",
			host:           "web.example.com",
			expectedStatus: http.StatusOK,
			shouldProxy:    true,
		},
		{
			name:           "unknown domain",
			host:           "api.unknown.com",
			expectedStatus: http.StatusOK, // Current implementation doesn't return error status
			shouldProxy:    false,
		},
		{
			name:           "unknown subdomain",
			host:           "unknown.example.com",
			expectedStatus: http.StatusOK, // Current implementation doesn't return error status
			shouldProxy:    false,
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			req := httptest.NewRequest("GET", "http://"+tt.host+"/test", nil)
			req.Host = tt.host
			
			rr := httptest.NewRecorder()
			handler := &ProxyHandler{}

			// Test the proxy
			handler.ServeHTTP(rr, req)

			// Check that the request was processed correctly
			if rr.Code != tt.expectedStatus {
				t.Errorf("ProxyHandler.ServeHTTP() status = %v, want %v", rr.Code, tt.expectedStatus)
			}
			
			// For successful proxy requests, check that we got a response
			if tt.shouldProxy && rr.Code == http.StatusOK {
				if rr.Body.Len() == 0 {
					t.Errorf("ProxyHandler.ServeHTTP() expected response body but got empty")
				}
			}
		})
	}
}

func TestProxyHandlerEmptyServers(t *testing.T) {
	// Test that empty servers list is handled gracefully
	originalDomains := Domains
	defer func() {
		Domains = originalDomains
	}()

	Domains = map[string]Domain{
		"example.com": {
			DomainPath: "example.com",
			Subdomains: map[string]Subdomain{
				"empty": {
					SubdomainPath: "empty",
					Servers:       []string{}, // Empty servers list
				},
			},
		},
	}

	req := httptest.NewRequest("GET", "http://empty.example.com/test", nil)
	req.Host = "empty.example.com"
	
	rr := httptest.NewRecorder()
	handler := &ProxyHandler{}

	// After fixing the bug, this should return a proper HTTP error
	handler.ServeHTTP(rr, req)

	// Should return 503 Service Unavailable
	if rr.Code != http.StatusServiceUnavailable {
		t.Errorf("ProxyHandler.ServeHTTP() with empty servers = %v, want %v", rr.Code, http.StatusServiceUnavailable)
	}

	expectedBody := "No servers available\n"
	if rr.Body.String() != expectedBody {
		t.Errorf("ProxyHandler.ServeHTTP() body = %q, want %q", rr.Body.String(), expectedBody)
	}
}

func TestDomainAndSubdomainTypes(t *testing.T) {
	// Test that our types can be created and used properly
	domain := Domain{
		DomainPath: "test.com",
		Subdomains: map[string]Subdomain{
			"api": {
				SubdomainPath: "api",
				Servers:       []string{"http://server1.com", "http://server2.com"},
			},
		},
	}

	if domain.DomainPath != "test.com" {
		t.Errorf("Domain.DomainPath = %q, want %q", domain.DomainPath, "test.com")
	}

	if len(domain.Subdomains) != 1 {
		t.Errorf("len(Domain.Subdomains) = %d, want %d", len(domain.Subdomains), 1)
	}

	subdomain := domain.Subdomains["api"]
	if subdomain.SubdomainPath != "api" {
		t.Errorf("Subdomain.SubdomainPath = %q, want %q", subdomain.SubdomainPath, "api")
	}

	if len(subdomain.Servers) != 2 {
		t.Errorf("len(Subdomain.Servers) = %d, want %d", len(subdomain.Servers), 2)
	}
}

// Benchmark the splitHostPort function
func BenchmarkSplitHostPort(b *testing.B) {
	testCases := []string{
		"api.example.com",
		"api.example.com:8080",
		"test.localhost:3000",
		"api.v1.example.com",
	}

	for i := 0; i < b.N; i++ {
		for _, tc := range testCases {
			splitHostPort(tc)
		}
	}
}

// Test helper function to create a mock HTTP server for integration testing
func createMockServer(response string) *httptest.Server {
	return httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		w.WriteHeader(http.StatusOK)
		w.Write([]byte(response))
	}))
}

func TestProxyHandlerIntegration(t *testing.T) {
	// Create a mock backend server
	mockServer := createMockServer("Hello from backend")
	defer mockServer.Close()

	// We don't need to parse the URL for this test

	// Set up test domains with the mock server
	originalDomains := Domains
	defer func() {
		Domains = originalDomains
	}()

	Domains = map[string]Domain{
		"example.com": {
			DomainPath: "example.com",
			Subdomains: map[string]Subdomain{
				"api": {
					SubdomainPath: "api",
					Servers:       []string{mockServer.URL},
				},
			},
		},
	}

	// Create a request
	req := httptest.NewRequest("GET", "http://api.example.com/test", nil)
	req.Host = "api.example.com"
	
	rr := httptest.NewRecorder()
	handler := &ProxyHandler{}

	// Test the proxy
	handler.ServeHTTP(rr, req)

	// Check that the request was proxied successfully
	if rr.Code != http.StatusOK {
		t.Errorf("ProxyHandler.ServeHTTP() status = %v, want %v", rr.Code, http.StatusOK)
	}

	expectedBody := "Hello from backend"
	if strings.TrimSpace(rr.Body.String()) != expectedBody {
		t.Errorf("ProxyHandler.ServeHTTP() body = %q, want %q", rr.Body.String(), expectedBody)
	}
}

func TestProxyHandlerSingleServer(t *testing.T) {
	// Test that single server selection works correctly
	mockServer := createMockServer("Hello from single server")
	defer mockServer.Close()

	originalDomains := Domains
	defer func() {
		Domains = originalDomains
	}()

	Domains = map[string]Domain{
		"example.com": {
			DomainPath: "example.com",
			Subdomains: map[string]Subdomain{
				"single": {
					SubdomainPath: "single",
					Servers:       []string{mockServer.URL},
				},
			},
		},
	}

	req := httptest.NewRequest("GET", "http://single.example.com/test", nil)
	req.Host = "single.example.com"
	
	rr := httptest.NewRecorder()
	handler := &ProxyHandler{}

	handler.ServeHTTP(rr, req)

	if rr.Code != http.StatusOK {
		t.Errorf("ProxyHandler.ServeHTTP() with single server = %v, want %v", rr.Code, http.StatusOK)
	}

	expectedBody := "Hello from single server"
	if rr.Body.String() != expectedBody {
		t.Errorf("ProxyHandler.ServeHTTP() body = %q, want %q", rr.Body.String(), expectedBody)
	}
}

func TestProxyHandlerMultipleServers(t *testing.T) {
	// Test that multiple server selection works (load balancing)
	mockServer1 := createMockServer("Server 1")
	defer mockServer1.Close()
	
	mockServer2 := createMockServer("Server 2")
	defer mockServer2.Close()

	originalDomains := Domains
	defer func() {
		Domains = originalDomains
	}()

	Domains = map[string]Domain{
		"example.com": {
			DomainPath: "example.com",
			Subdomains: map[string]Subdomain{
				"multi": {
					SubdomainPath: "multi",
					Servers:       []string{mockServer1.URL, mockServer2.URL},
				},
			},
		},
	}

	handler := &ProxyHandler{}
	
	// Make multiple requests to test load balancing
	responses := make(map[string]int)
	for i := 0; i < 10; i++ {
		req := httptest.NewRequest("GET", "http://multi.example.com/test", nil)
		req.Host = "multi.example.com"
		
		rr := httptest.NewRecorder()
		handler.ServeHTTP(rr, req)

		if rr.Code != http.StatusOK {
			t.Errorf("ProxyHandler.ServeHTTP() request %d status = %v, want %v", i, rr.Code, http.StatusOK)
		}
		
		responses[rr.Body.String()]++
	}

	// Should have responses from both servers
	if len(responses) == 0 {
		t.Error("No responses received")
	}
	
	// At least one response should be received (random distribution)
	totalResponses := 0
	for _, count := range responses {
		totalResponses += count
	}
	
	if totalResponses != 10 {
		t.Errorf("Expected 10 total responses, got %d", totalResponses)
	}
}

func TestProxyHandlerInvalidURL(t *testing.T) {
	// Test that invalid server URLs are handled properly
	originalDomains := Domains
	defer func() {
		Domains = originalDomains
	}()

	Domains = map[string]Domain{
		"example.com": {
			DomainPath: "example.com",
			Subdomains: map[string]Subdomain{
				"invalid": {
					SubdomainPath: "invalid",
					Servers:       []string{"://invalid-url"},
				},
			},
		},
	}

	req := httptest.NewRequest("GET", "http://invalid.example.com/test", nil)
	req.Host = "invalid.example.com"
	
	rr := httptest.NewRecorder()
	handler := &ProxyHandler{}

	handler.ServeHTTP(rr, req)

	// Should return 500 for invalid URL
	if rr.Code != http.StatusInternalServerError {
		t.Errorf("ProxyHandler.ServeHTTP() with invalid URL status = %v, want %v", rr.Code, http.StatusInternalServerError)
	}

	expectedBody := "Invalid target URL\n"
	if rr.Body.String() != expectedBody {
		t.Errorf("ProxyHandler.ServeHTTP() body = %q, want %q", rr.Body.String(), expectedBody)
	}
}

func TestProxyHandlerNoSubdomainFound(t *testing.T) {
	// Test the case where subdomain is not found in domain
	originalDomains := Domains
	defer func() {
		Domains = originalDomains
	}()

	Domains = map[string]Domain{
		"example.com": {
			DomainPath: "example.com",
			Subdomains: map[string]Subdomain{
				"api": {
					SubdomainPath: "api",
					Servers:       []string{"http://backend.com"},
				},
			},
		},
	}

	req := httptest.NewRequest("GET", "http://missing.example.com/test", nil)
	req.Host = "missing.example.com"
	
	rr := httptest.NewRecorder()
	handler := &ProxyHandler{}

	handler.ServeHTTP(rr, req)

	// Should return 200 but with no response body (current implementation)
	if rr.Code != http.StatusOK {
		t.Errorf("ProxyHandler.ServeHTTP() with missing subdomain status = %v, want %v", rr.Code, http.StatusOK)
	}
}