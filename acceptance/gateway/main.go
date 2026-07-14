package main

import (
	"bufio"
	"bytes"
	"context"
	"crypto/rand"
	"crypto/rsa"
	"crypto/sha1"
	"crypto/sha256"
	"crypto/tls"
	"crypto/x509"
	"crypto/x509/pkix"
	"encoding/base64"
	"encoding/hex"
	"encoding/json"
	"encoding/pem"
	"errors"
	"fmt"
	"io"
	"math/big"
	"net"
	"net/http"
	"os"
	"path/filepath"
	"strings"
	"time"
)

const domain = "bill-app.stackctl.localhost"

type probeResult struct {
	HTTP1          bool   `json:"http1"`
	HTTP2          bool   `json:"http2"`
	Redirect       bool   `json:"redirect"`
	WebSocket      bool   `json:"websocket"`
	Streaming      bool   `json:"streaming"`
	LargeBody      bool   `json:"large_body"`
	ConfigRevision string `json:"config_revision"`
}

type continuityResult struct {
	Streaming bool `json:"streaming"`
	WebSocket bool `json:"websocket"`
}

func main() {
	if err := run(os.Args[1:]); err != nil {
		fmt.Fprintln(os.Stderr, err)
		os.Exit(1)
	}
}

func run(arguments []string) error {
	if len(arguments) == 0 {
		return errors.New("expected cert, serve, wait, wait-file, probe, or continuity command")
	}
	switch arguments[0] {
	case "cert":
		if len(arguments) != 2 {
			return errors.New("cert requires one output directory")
		}
		return generateCertificates(arguments[1])
	case "serve":
		return serve()
	case "wait":
		if len(arguments) != 2 {
			return errors.New("wait requires one host:port")
		}
		return waitForPort(arguments[1])
	case "wait-file":
		if len(arguments) != 2 {
			return errors.New("wait-file requires one path")
		}
		return waitForFile(arguments[1])
	case "probe":
		if len(arguments) != 5 {
			return errors.New("probe requires HTTP port, HTTPS port, CA path, and config revision")
		}
		return probe(arguments[1], arguments[2], arguments[3], arguments[4])
	case "continuity":
		if len(arguments) != 5 {
			return errors.New("continuity requires HTTPS port, CA path, ready path, and release path")
		}
		return probeReloadContinuity(arguments[1], arguments[2], arguments[3], arguments[4])
	default:
		return fmt.Errorf("unknown command %q", arguments[0])
	}
}

func generateCertificates(directory string) error {
	if err := os.MkdirAll(directory, 0o700); err != nil {
		return err
	}
	now := time.Now()
	caKey, err := rsa.GenerateKey(rand.Reader, 2048)
	if err != nil {
		return err
	}
	ca := &x509.Certificate{
		SerialNumber:          big.NewInt(1),
		Subject:               pkix.Name{CommonName: "Stackctl gateway acceptance CA"},
		NotBefore:             now.Add(-time.Hour),
		NotAfter:              now.Add(24 * time.Hour),
		IsCA:                  true,
		BasicConstraintsValid: true,
		KeyUsage:              x509.KeyUsageCertSign | x509.KeyUsageDigitalSignature,
	}
	caDER, err := x509.CreateCertificate(rand.Reader, ca, ca, &caKey.PublicKey, caKey)
	if err != nil {
		return err
	}
	leafKey, err := rsa.GenerateKey(rand.Reader, 2048)
	if err != nil {
		return err
	}
	leaf := &x509.Certificate{
		SerialNumber: big.NewInt(2),
		Subject:      pkix.Name{CommonName: "*.stackctl.localhost"},
		DNSNames:     []string{"*.stackctl.localhost"},
		NotBefore:    now.Add(-time.Hour),
		NotAfter:     now.Add(12 * time.Hour),
		KeyUsage:     x509.KeyUsageDigitalSignature | x509.KeyUsageKeyEncipherment,
		ExtKeyUsage:  []x509.ExtKeyUsage{x509.ExtKeyUsageServerAuth},
	}
	leafDER, err := x509.CreateCertificate(rand.Reader, leaf, ca, &leafKey.PublicKey, caKey)
	if err != nil {
		return err
	}
	caPEM := pem.EncodeToMemory(&pem.Block{Type: "CERTIFICATE", Bytes: caDER})
	leafPEM := append(
		pem.EncodeToMemory(&pem.Block{Type: "CERTIFICATE", Bytes: leafDER}),
		caPEM...,
	)
	keyPEM := pem.EncodeToMemory(&pem.Block{
		Type:  "RSA PRIVATE KEY",
		Bytes: x509.MarshalPKCS1PrivateKey(leafKey),
	})
	for name, contents := range map[string][]byte{
		"ca.pem": caPEM, "cert.pem": leafPEM, "key.pem": keyPEM,
	} {
		if err := os.WriteFile(filepath.Join(directory, name), contents, 0o600); err != nil {
			return err
		}
	}
	return nil
}

func serve() error {
	mux := http.NewServeMux()
	mux.HandleFunc("/protocol", func(writer http.ResponseWriter, request *http.Request) {
		_, _ = io.WriteString(writer, request.Proto)
	})
	mux.HandleFunc("/large", func(writer http.ResponseWriter, request *http.Request) {
		digest := sha256.New()
		size, err := io.Copy(digest, request.Body)
		if err != nil {
			http.Error(writer, err.Error(), http.StatusBadRequest)
			return
		}
		_, _ = fmt.Fprintf(writer, "%d:%s", size, hex.EncodeToString(digest.Sum(nil)))
	})
	mux.HandleFunc("/stream", func(writer http.ResponseWriter, _ *http.Request) {
		_, _ = io.WriteString(writer, "first\n")
		if flusher, ok := writer.(http.Flusher); ok {
			flusher.Flush()
		}
		time.Sleep(500 * time.Millisecond)
		_, _ = io.WriteString(writer, "second\n")
	})
	mux.HandleFunc("/reload-stream", func(writer http.ResponseWriter, _ *http.Request) {
		_, _ = io.WriteString(writer, "before-reload\n")
		if flusher, ok := writer.(http.Flusher); ok {
			flusher.Flush()
		}
		time.Sleep(5 * time.Second)
		_, _ = io.WriteString(writer, "after-reload\n")
	})
	mux.HandleFunc("/ws", websocketEcho)
	mux.HandleFunc("/ready", func(writer http.ResponseWriter, _ *http.Request) {
		writer.WriteHeader(http.StatusNoContent)
	})
	server := &http.Server{
		Addr:              ":8080",
		Handler:           mux,
		ReadHeaderTimeout: 5 * time.Second,
	}
	return server.ListenAndServe()
}

func websocketEcho(writer http.ResponseWriter, request *http.Request) {
	key := request.Header.Get("Sec-WebSocket-Key")
	hijacker, ok := writer.(http.Hijacker)
	if !ok || key == "" {
		http.Error(writer, "websocket upgrade required", http.StatusBadRequest)
		return
	}
	connection, buffer, err := hijacker.Hijack()
	if err != nil {
		return
	}
	defer connection.Close()
	accept := sha1.Sum([]byte(key + "258EAFA5-E914-47DA-95CA-C5AB0DC85B11"))
	_, _ = fmt.Fprintf(
		buffer,
		"HTTP/1.1 101 Switching Protocols\r\nUpgrade: websocket\r\nConnection: Upgrade\r\nSec-WebSocket-Accept: %s\r\n\r\n",
		base64.StdEncoding.EncodeToString(accept[:]),
	)
	if err := buffer.Flush(); err != nil {
		return
	}
	payload, err := readWebSocketFrame(buffer.Reader)
	if err != nil || len(payload) > 125 {
		return
	}
	_, _ = connection.Write(append([]byte{0x81, byte(len(payload))}, payload...))
}

func readWebSocketFrame(reader io.Reader) ([]byte, error) {
	header := make([]byte, 2)
	if _, err := io.ReadFull(reader, header); err != nil {
		return nil, err
	}
	if header[1]&0x80 == 0 || header[1]&0x7f > 125 {
		return nil, errors.New("expected one masked short websocket frame")
	}
	mask := make([]byte, 4)
	if _, err := io.ReadFull(reader, mask); err != nil {
		return nil, err
	}
	payload := make([]byte, int(header[1]&0x7f))
	if _, err := io.ReadFull(reader, payload); err != nil {
		return nil, err
	}
	for index := range payload {
		payload[index] ^= mask[index%len(mask)]
	}
	return payload, nil
}

func waitForPort(address string) error {
	deadline := time.Now().Add(30 * time.Second)
	for time.Now().Before(deadline) {
		connection, err := net.DialTimeout("tcp", address, 250*time.Millisecond)
		if err == nil {
			_ = connection.Close()
			return nil
		}
		time.Sleep(100 * time.Millisecond)
	}
	return fmt.Errorf("gateway did not listen on %s within 30 seconds", address)
}

func waitForFile(path string) error {
	deadline := time.Now().Add(30 * time.Second)
	for time.Now().Before(deadline) {
		if _, err := os.Stat(path); err == nil {
			return nil
		} else if !errors.Is(err, os.ErrNotExist) {
			return err
		}
		time.Sleep(25 * time.Millisecond)
	}
	return fmt.Errorf("file %s did not appear within 30 seconds", path)
}

func probe(httpPort, httpsPort, caPath, expectedRevision string) error {
	caPEM, err := os.ReadFile(caPath)
	if err != nil {
		return err
	}
	roots := x509.NewCertPool()
	if !roots.AppendCertsFromPEM(caPEM) {
		return errors.New("could not parse acceptance CA")
	}
	result := probeResult{}
	if err := probeHTTPVersions(httpsPort, roots, expectedRevision, &result); err != nil {
		return err
	}
	if err := probeRedirect(httpPort, &result); err != nil {
		return err
	}
	if err := probeLargeBody(httpsPort, roots, &result); err != nil {
		return err
	}
	if err := probeStreaming(httpsPort, roots, &result); err != nil {
		return err
	}
	if err := probeWebSocket(httpsPort, roots, &result); err != nil {
		return err
	}
	return json.NewEncoder(os.Stdout).Encode(result)
}

func tlsConfig(roots *x509.CertPool) *tls.Config {
	return &tls.Config{RootCAs: roots, ServerName: domain, MinVersion: tls.VersionTLS12}
}

func request(client *http.Client, method, url string, body io.Reader) (*http.Response, error) {
	request, err := http.NewRequestWithContext(context.Background(), method, url, body)
	if err != nil {
		return nil, err
	}
	request.Host = domain
	return client.Do(request)
}

func probeHTTPVersions(
	port string,
	roots *x509.CertPool,
	expectedRevision string,
	result *probeResult,
) error {
	url := "https://127.0.0.1:" + port + "/protocol"
	http1Transport := &http.Transport{
		TLSClientConfig: tlsConfig(roots),
		TLSNextProto:    map[string]func(string, *tls.Conn) http.RoundTripper{},
	}
	http1 := &http.Client{Transport: http1Transport, Timeout: 10 * time.Second}
	response, err := request(http1, http.MethodGet, url, nil)
	if err != nil {
		return err
	}
	body, err := io.ReadAll(response.Body)
	_ = response.Body.Close()
	if err != nil || response.ProtoMajor != 1 || string(body) != "HTTP/1.1" {
		return fmt.Errorf("HTTP/1.1 probe failed: protocol=%s body=%q error=%v", response.Proto, body, err)
	}
	if revision := response.Header.Get("X-Stackctl-Revision"); revision != expectedRevision {
		return fmt.Errorf("config revision mismatch: expected=%q actual=%q", expectedRevision, revision)
	}
	result.HTTP1 = true
	result.ConfigRevision = expectedRevision
	http2 := &http.Client{Transport: &http.Transport{
		TLSClientConfig:   tlsConfig(roots),
		ForceAttemptHTTP2: true,
	}, Timeout: 10 * time.Second}
	response, err = request(http2, http.MethodGet, url, nil)
	if err != nil {
		return err
	}
	body, err = io.ReadAll(response.Body)
	_ = response.Body.Close()
	if err != nil || response.ProtoMajor != 2 || string(body) != "HTTP/1.1" {
		return fmt.Errorf("HTTP/2 probe failed: protocol=%s body=%q error=%v", response.Proto, body, err)
	}
	result.HTTP2 = true
	return nil
}

func probeRedirect(port string, result *probeResult) error {
	client := &http.Client{
		CheckRedirect: func(_ *http.Request, _ []*http.Request) error { return http.ErrUseLastResponse },
		Timeout:       10 * time.Second,
	}
	response, err := request(client, http.MethodGet, "http://127.0.0.1:"+port+"/protocol", nil)
	if err != nil {
		return err
	}
	_ = response.Body.Close()
	if response.StatusCode != http.StatusPermanentRedirect || response.Header.Get("Location") != "https://"+domain+"/protocol" {
		return fmt.Errorf("redirect probe failed: status=%d location=%q", response.StatusCode, response.Header.Get("Location"))
	}
	result.Redirect = true
	return nil
}

func probeLargeBody(port string, roots *x509.CertPool, result *probeResult) error {
	payload := bytes.Repeat([]byte("stackctl-large-body\n"), 1<<19)
	digest := sha256.Sum256(payload)
	client := &http.Client{Transport: &http.Transport{TLSClientConfig: tlsConfig(roots)}, Timeout: 30 * time.Second}
	response, err := request(client, http.MethodPost, "https://127.0.0.1:"+port+"/large", bytes.NewReader(payload))
	if err != nil {
		return err
	}
	body, err := io.ReadAll(response.Body)
	_ = response.Body.Close()
	expected := fmt.Sprintf("%d:%s", len(payload), hex.EncodeToString(digest[:]))
	if err != nil || string(body) != expected {
		return fmt.Errorf("large body probe failed: body=%q error=%v", body, err)
	}
	result.LargeBody = true
	return nil
}

func probeStreaming(port string, roots *x509.CertPool, result *probeResult) error {
	client := &http.Client{Transport: &http.Transport{TLSClientConfig: tlsConfig(roots)}, Timeout: 10 * time.Second}
	response, err := request(client, http.MethodGet, "https://127.0.0.1:"+port+"/stream", nil)
	if err != nil {
		return err
	}
	defer response.Body.Close()
	start := time.Now()
	first := make([]byte, len("first\n"))
	if _, err := io.ReadFull(response.Body, first); err != nil {
		return err
	}
	if string(first) != "first\n" || time.Since(start) >= 400*time.Millisecond {
		return fmt.Errorf("streaming first chunk was buffered for %s", time.Since(start))
	}
	rest, err := io.ReadAll(response.Body)
	if err != nil || string(rest) != "second\n" {
		return fmt.Errorf("streaming final chunk failed: body=%q error=%v", rest, err)
	}
	result.Streaming = true
	return nil
}

func probeWebSocket(port string, roots *x509.CertPool, result *probeResult) error {
	connection, reader, err := openWebSocket(port, roots)
	if err != nil {
		return err
	}
	defer connection.Close()
	if err := exchangeWebSocket(connection, reader); err != nil {
		return err
	}
	result.WebSocket = true
	return nil
}

func openWebSocket(port string, roots *x509.CertPool) (*tls.Conn, *bufio.Reader, error) {
	connection, err := tls.Dial("tcp", "127.0.0.1:"+port, &tls.Config{
		RootCAs: roots, ServerName: domain, MinVersion: tls.VersionTLS12,
		NextProtos: []string{"http/1.1"},
	})
	if err != nil {
		return nil, nil, err
	}
	keyBytes := make([]byte, 16)
	if _, err := rand.Read(keyBytes); err != nil {
		_ = connection.Close()
		return nil, nil, err
	}
	key := base64.StdEncoding.EncodeToString(keyBytes)
	_, err = fmt.Fprintf(connection, "GET /ws HTTP/1.1\r\nHost: %s\r\nUpgrade: websocket\r\nConnection: Upgrade\r\nSec-WebSocket-Key: %s\r\nSec-WebSocket-Version: 13\r\n\r\n", domain, key)
	if err != nil {
		_ = connection.Close()
		return nil, nil, err
	}
	reader := bufio.NewReader(connection)
	status, err := reader.ReadString('\n')
	if err != nil || !strings.Contains(status, " 101 ") {
		_ = connection.Close()
		return nil, nil, fmt.Errorf("websocket upgrade failed: status=%q error=%v", status, err)
	}
	for {
		line, err := reader.ReadString('\n')
		if err != nil {
			_ = connection.Close()
			return nil, nil, err
		}
		if line == "\r\n" {
			break
		}
	}
	return connection, reader, nil
}

func exchangeWebSocket(connection net.Conn, reader *bufio.Reader) error {
	payload := []byte("stackctl-websocket")
	mask := []byte{1, 2, 3, 4}
	frame := []byte{0x81, 0x80 | byte(len(payload)), 1, 2, 3, 4}
	for index, value := range payload {
		frame = append(frame, value^mask[index%len(mask)])
	}
	if _, err := connection.Write(frame); err != nil {
		return err
	}
	header := make([]byte, 2)
	if _, err := io.ReadFull(reader, header); err != nil {
		return err
	}
	echo := make([]byte, int(header[1]&0x7f))
	if header[0] != 0x81 || header[1]&0x80 != 0 {
		return errors.New("websocket echo returned an invalid frame")
	}
	if _, err := io.ReadFull(reader, echo); err != nil {
		return err
	}
	if !bytes.Equal(echo, payload) {
		return fmt.Errorf("websocket echo mismatch: %q", echo)
	}
	return nil
}

func probeReloadContinuity(httpsPort, caPath, readyPath, releasePath string) error {
	caPEM, err := os.ReadFile(caPath)
	if err != nil {
		return err
	}
	roots := x509.NewCertPool()
	if !roots.AppendCertsFromPEM(caPEM) {
		return errors.New("could not parse acceptance CA")
	}
	connection, reader, err := openWebSocket(httpsPort, roots)
	if err != nil {
		return err
	}
	defer connection.Close()
	client := &http.Client{
		Transport: &http.Transport{TLSClientConfig: tlsConfig(roots)},
		Timeout:   20 * time.Second,
	}
	response, err := request(
		client,
		http.MethodGet,
		"https://127.0.0.1:"+httpsPort+"/reload-stream",
		nil,
	)
	if err != nil {
		return err
	}
	defer response.Body.Close()
	first := make([]byte, len("before-reload\n"))
	if _, err := io.ReadFull(response.Body, first); err != nil || string(first) != "before-reload\n" {
		return fmt.Errorf("reload stream did not start: body=%q error=%v", first, err)
	}
	if err := os.WriteFile(readyPath, []byte("ready\n"), 0o600); err != nil {
		return err
	}
	if err := waitForFile(releasePath); err != nil {
		return err
	}
	second, err := io.ReadAll(response.Body)
	if err != nil || string(second) != "after-reload\n" {
		return fmt.Errorf("reload stream continuity failed: body=%q error=%v", second, err)
	}
	if err := exchangeWebSocket(connection, reader); err != nil {
		return fmt.Errorf("reload websocket continuity failed: %w", err)
	}
	return json.NewEncoder(os.Stdout).Encode(continuityResult{Streaming: true, WebSocket: true})
}
