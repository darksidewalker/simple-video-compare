package server

import (
	"fmt"
	"net/http"
	"os"
)

func HandleShutdown(w http.ResponseWriter, r *http.Request) {
	if r.Method != http.MethodPost {
		http.Error(w, "Method not allowed", http.StatusMethodNotAllowed)
		return
	}

	fmt.Println("DaSiWa Simple Video Compare shutting down via Quit button...")
	
	w.WriteHeader(http.StatusOK)
	fmt.Fprint(w, "OK")
	
	// Trigger immediate exit after response
	go func() {
		os.Exit(0)
	}()
}
