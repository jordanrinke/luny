/* @dose
purpose: Sample Go fixture for testing the luny Go parser.
    This file contains various Go constructs including structs, interfaces,
    functions, and methods to verify extraction works correctly.

when-editing:
    - !Keep both exported (uppercase) and unexported (lowercase) items for testing
    - Maintain the mix of sync and async patterns

invariants:
    - Exported items must start with uppercase letters
    - Unexported items must start with lowercase letters
    - Include examples of interfaces, structs, and functions

do-not:
    - Remove any exports without updating corresponding tests
    - Use cgo in this fixture

gotchas:
    - Go determines visibility by the first letter case
    - Methods with pointer receivers are common patterns
    - Interface satisfaction is implicit
*/

package sample

import (
	"encoding/json"
	"fmt"
	"io/ioutil"
	"os"
	"path/filepath"
	"sync"
	"time"
)

// Exported constants
const (
	Version        = "1.0.0"
	DefaultTimeout = 30 * time.Second
	MaxRetries     = 3
)

// unexported constant
const internalBufferSize = 1024

// Exported variable (for parser coverage of var_declaration)
var DefaultConfig *UserConfig

// Exported type alias
type UserID string

// Exported interface
type Repository interface {
	Get(id string) (interface{}, error)
	Save(item interface{}) error
	Delete(id string) error
}

// Exported struct
type UserConfig struct {
	ID       UserID            `json:"id"`
	Name     string            `json:"name"`
	Email    string            `json:"email,omitempty"`
	Settings map[string]any    `json:"settings,omitempty"`
}

// Exported struct with methods
type UserService struct {
	dataDir string
	cache   map[string]*UserConfig
	mu      sync.RWMutex
}

// Constructor function (exported)
func NewUserService(dataDir string) *UserService {
	return &UserService{
		dataDir: dataDir,
		cache:   make(map[string]*UserConfig),
	}
}

// Exported method with pointer receiver
func (s *UserService) Get(id string) (*UserConfig, error) {
	s.mu.RLock()
	if user, ok := s.cache[id]; ok {
		s.mu.RUnlock()
		return user, nil
	}
	s.mu.RUnlock()

	filePath := filepath.Join(s.dataDir, id+".json")
	data, err := ioutil.ReadFile(filePath)
	if err != nil {
		if os.IsNotExist(err) {
			return nil, nil
		}
		return nil, fmt.Errorf("failed to read user file: %w", err)
	}

	var user UserConfig
	if err := json.Unmarshal(data, &user); err != nil {
		return nil, fmt.Errorf("failed to parse user file: %w", err)
	}

	s.mu.Lock()
	s.cache[id] = &user
	s.mu.Unlock()

	return &user, nil
}

// Exported method
func (s *UserService) Save(user *UserConfig) error {
	if err := s.validate(user); err != nil {
		return err
	}

	if err := os.MkdirAll(s.dataDir, 0755); err != nil {
		return fmt.Errorf("failed to create data directory: %w", err)
	}

	data, err := json.MarshalIndent(user, "", "  ")
	if err != nil {
		return fmt.Errorf("failed to serialize user: %w", err)
	}

	filePath := filepath.Join(s.dataDir, string(user.ID)+".json")
	if err := ioutil.WriteFile(filePath, data, 0644); err != nil {
		return fmt.Errorf("failed to write user file: %w", err)
	}

	s.mu.Lock()
	s.cache[string(user.ID)] = user
	s.mu.Unlock()

	return nil
}

// Exported method
func (s *UserService) Delete(id string) error {
	filePath := filepath.Join(s.dataDir, id+".json")
	if err := os.Remove(filePath); err != nil && !os.IsNotExist(err) {
		return fmt.Errorf("failed to delete user file: %w", err)
	}

	s.mu.Lock()
	delete(s.cache, id)
	s.mu.Unlock()

	return nil
}

// Exported method
func (s *UserService) List() ([]*UserConfig, error) {
	files, err := ioutil.ReadDir(s.dataDir)
	if err != nil {
		if os.IsNotExist(err) {
			return nil, nil
		}
		return nil, fmt.Errorf("failed to read data directory: %w", err)
	}

	var users []*UserConfig
	for _, file := range files {
		if filepath.Ext(file.Name()) != ".json" {
			continue
		}
		id := file.Name()[:len(file.Name())-5]
		user, err := s.Get(id)
		if err != nil {
			return nil, err
		}
		if user != nil {
			users = append(users, user)
		}
	}

	return users, nil
}

// unexported method (private)
func (s *UserService) validate(user *UserConfig) error {
	if user.ID == "" {
		return fmt.Errorf("user ID is required")
	}
	if user.Name == "" {
		return fmt.Errorf("user name is required")
	}
	return nil
}

// Exported function
func CreateUser(name, email string) *UserConfig {
	return &UserConfig{
		ID:       UserID(generateID()),
		Name:     name,
		Email:    email,
		Settings: make(map[string]any),
	}
}

// Exported function with multiple return values
func ValidateEmail(email string) (bool, error) {
	if email == "" {
		return false, fmt.Errorf("email is required")
	}
	// Simple validation
	for i, c := range email {
		if c == '@' && i > 0 && i < len(email)-1 {
			return true, nil
		}
	}
	return false, fmt.Errorf("invalid email format")
}

// unexported function (private)
func generateID() string {
	return fmt.Sprintf("%d", time.Now().UnixNano())
}

// Exported interface for testing
type Logger interface {
	Info(msg string)
	Error(msg string)
	Debug(msg string)
}

// Exported struct implementing Logger
type ConsoleLogger struct {
	prefix string
}

func NewConsoleLogger(prefix string) *ConsoleLogger {
	return &ConsoleLogger{prefix: prefix}
}

func (l *ConsoleLogger) Info(msg string) {
	fmt.Printf("[%s] INFO: %s\n", l.prefix, msg)
}

func (l *ConsoleLogger) Error(msg string) {
	fmt.Printf("[%s] ERROR: %s\n", l.prefix, msg)
}

func (l *ConsoleLogger) Debug(msg string) {
	fmt.Printf("[%s] DEBUG: %s\n", l.prefix, msg)
}

// Exported generic function (Go 1.18+)
func Filter[T any](items []T, predicate func(T) bool) []T {
	var result []T
	for _, item := range items {
		if predicate(item) {
			result = append(result, item)
		}
	}
	return result
}

// Exported generic struct
type Cache[K comparable, V any] struct {
	data map[K]V
	mu   sync.RWMutex
}

func NewCache[K comparable, V any]() *Cache[K, V] {
	return &Cache[K, V]{
		data: make(map[K]V),
	}
}

func (c *Cache[K, V]) Get(key K) (V, bool) {
	c.mu.RLock()
	defer c.mu.RUnlock()
	val, ok := c.data[key]
	return val, ok
}

func (c *Cache[K, V]) Set(key K, value V) {
	c.mu.Lock()
	defer c.mu.Unlock()
	c.data[key] = value
}
