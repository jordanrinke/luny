"""@toon
purpose: Sample Python fixture for testing the luny Python parser.
    This file contains various Python constructs including classes, functions,
    decorators, and type hints to verify extraction works correctly.

when-editing:
    - !Keep all export types represented for comprehensive testing
    - Maintain the mix of sync and async functions
    - Preserve decorator patterns

invariants:
    - All public items must have clear, testable names
    - Import statements should cover all supported patterns
    - Private functions (underscore prefix) should not be exported

do-not:
    - Remove any exports without updating corresponding tests
    - Use relative imports in this fixture

gotchas:
    - Python uses underscore convention for private items
    - Decorators affect function signatures
"""

from typing import Optional, List, Dict, Any, TypeVar, Generic
from dataclasses import dataclass, field
from abc import ABC, abstractmethod
from functools import wraps
import asyncio
import json
import os

# Type variables
T = TypeVar('T')
K = TypeVar('K')
V = TypeVar('V')


# Decorator
def log_calls(func):
    """Decorator that logs function calls."""
    @wraps(func)
    def wrapper(*args, **kwargs):
        print(f"Calling {func.__name__}")
        result = func(*args, **kwargs)
        print(f"{func.__name__} returned {result}")
        return result
    return wrapper


# Dataclass export
@dataclass
class UserConfig:
    """Configuration for a user."""
    id: str
    name: str
    email: Optional[str] = None
    settings: Dict[str, Any] = field(default_factory=dict)


# Abstract base class
class BaseService(ABC, Generic[T]):
    """Abstract base class for services."""

    @abstractmethod
    def get(self, id: str) -> Optional[T]:
        """Get an item by ID."""
        pass

    @abstractmethod
    def save(self, item: T) -> bool:
        """Save an item."""
        pass


# Concrete class
class UserService(BaseService[UserConfig]):
    """Service for managing users."""

    def __init__(self, data_dir: str = "data"):
        self.data_dir = data_dir
        self._cache: Dict[str, UserConfig] = {}

    def get(self, id: str) -> Optional[UserConfig]:
        """Get a user by ID."""
        if id in self._cache:
            return self._cache[id]

        filepath = os.path.join(self.data_dir, f"{id}.json")
        if os.path.exists(filepath):
            with open(filepath) as f:
                data = json.load(f)
                user = UserConfig(**data)
                self._cache[id] = user
                return user
        return None

    def save(self, user: UserConfig) -> bool:
        """Save a user to disk."""
        filepath = os.path.join(self.data_dir, f"{user.id}.json")
        try:
            with open(filepath, 'w') as f:
                json.dump(user.__dict__, f, indent=2)
            self._cache[user.id] = user
            return True
        except IOError:
            return False

    def _validate(self, user: UserConfig) -> bool:
        """Private validation method."""
        return len(user.id) > 0 and len(user.name) > 0


# Function exports
@log_calls
def validate_email(email: str) -> bool:
    """Validate an email address format."""
    return '@' in email and '.' in email.split('@')[1]


async def fetch_user_async(user_id: str) -> Optional[UserConfig]:
    """Async function to fetch a user."""
    await asyncio.sleep(0.1)  # Simulate network delay
    service = UserService()
    return service.get(user_id)


def create_user(name: str, email: Optional[str] = None) -> UserConfig:
    """Factory function to create a new user."""
    import uuid
    return UserConfig(
        id=str(uuid.uuid4()),
        name=name,
        email=email,
    )


# Constants
DEFAULT_TIMEOUT: int = 30
MAX_RETRIES: int = 3
VERSION: str = "1.0.0"


# Private function (should not be exported)
def _internal_helper() -> None:
    """Internal helper function."""
    pass


# Generic class
class Cache(Generic[K, V]):
    """Generic cache implementation."""

    def __init__(self, max_size: int = 100):
        self.max_size = max_size
        self._data: Dict[K, V] = {}

    def get(self, key: K) -> Optional[V]:
        return self._data.get(key)

    def set(self, key: K, value: V) -> None:
        if len(self._data) >= self.max_size:
            # Remove oldest entry
            oldest = next(iter(self._data))
            del self._data[oldest]
        self._data[key] = value
