package main

import (
  "log"
  "net/http"
  "os"
)

func main() {
  port := os.Getenv("PORT")
  if port == "" { port = "3000" }
  http.HandleFunc("/api/health", func(w http.ResponseWriter, r *http.Request) {
    w.Header().Set("Content-Type", "application/json")
    w.Write([]byte(`{"status":"ok"}`))
  })
  log.Printf("Listening on :%s", port)
  log.Fatal(http.ListenAndServe(":"+port, nil))
}
