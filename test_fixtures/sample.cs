/** @dose
purpose: Sample C# fixture for testing the luny C# parser.
    This file contains various C# constructs including classes, interfaces,
    records, and async patterns to verify extraction works correctly.

when-editing:
    - !Keep all visibility modifiers represented for comprehensive testing
    - Maintain the mix of sync and async methods

invariants:
    - All public items must have clear, testable names
    - Include examples of each visibility modifier

do-not:
    - Remove any exports without updating corresponding tests
    - Use partial classes in this fixture

gotchas:
    - C# uses explicit visibility modifiers (public, private, protected, internal)
    - Records are a special case of class
    - Primary constructors in records affect signature extraction
*/

using System;
using System.Collections.Generic;
using System.IO;
using System.Linq;
using System.Text.Json;
using System.Threading.Tasks;

namespace Luny.TestFixtures
{
    // Public enum
    public enum UserStatus
    {
        Active,
        Inactive,
        Pending,
        Suspended
    }

    // Public interface
    public interface IRepository<T> where T : class
    {
        Task<T?> GetByIdAsync(string id);
        Task<bool> SaveAsync(T entity);
        Task<IEnumerable<T>> GetAllAsync();
        Task<bool> DeleteAsync(string id);
    }

    // Public record (C# 9+)
    public record UserConfig(
        string Id,
        string Name,
        string? Email = null,
        Dictionary<string, object>? Settings = null
    );

    // Public class with interface implementation
    public class UserService : IRepository<UserConfig>
    {
        private readonly string _dataDir;
        private readonly Dictionary<string, UserConfig> _cache;

        public UserService(string dataDir = "data")
        {
            _dataDir = dataDir;
            _cache = new Dictionary<string, UserConfig>();
        }

        public async Task<UserConfig?> GetByIdAsync(string id)
        {
            if (_cache.TryGetValue(id, out var cached))
            {
                return cached;
            }

            var filePath = Path.Combine(_dataDir, $"{id}.json");
            if (!File.Exists(filePath))
            {
                return null;
            }

            var json = await File.ReadAllTextAsync(filePath);
            var user = JsonSerializer.Deserialize<UserConfig>(json);
            if (user != null)
            {
                _cache[id] = user;
            }
            return user;
        }

        public async Task<bool> SaveAsync(UserConfig user)
        {
            ValidateUser(user);

            Directory.CreateDirectory(_dataDir);
            var filePath = Path.Combine(_dataDir, $"{user.Id}.json");
            var json = JsonSerializer.Serialize(user, new JsonSerializerOptions
            {
                WriteIndented = true
            });

            await File.WriteAllTextAsync(filePath, json);
            _cache[user.Id] = user;
            return true;
        }

        public async Task<IEnumerable<UserConfig>> GetAllAsync()
        {
            var users = new List<UserConfig>();
            var files = Directory.GetFiles(_dataDir, "*.json");

            foreach (var file in files)
            {
                var id = Path.GetFileNameWithoutExtension(file);
                var user = await GetByIdAsync(id);
                if (user != null)
                {
                    users.Add(user);
                }
            }

            return users;
        }

        public Task<bool> DeleteAsync(string id)
        {
            var filePath = Path.Combine(_dataDir, $"{id}.json");
            if (File.Exists(filePath))
            {
                File.Delete(filePath);
                _cache.Remove(id);
                return Task.FromResult(true);
            }
            return Task.FromResult(false);
        }

        // Private validation method
        private void ValidateUser(UserConfig user)
        {
            if (string.IsNullOrEmpty(user.Id))
                throw new ArgumentException("User ID is required", nameof(user));
            if (string.IsNullOrEmpty(user.Name))
                throw new ArgumentException("User name is required", nameof(user));
        }

        // Protected method for subclasses
        protected virtual void OnUserSaved(UserConfig user)
        {
            // Hook for subclasses
        }

        // Internal method
        internal void ClearCache()
        {
            _cache.Clear();
        }
    }

    // Static utility class
    public static class StringExtensions
    {
        public static string ToCamelCase(this string str)
        {
            if (string.IsNullOrEmpty(str)) return str;
            return char.ToLower(str[0]) + str.Substring(1);
        }

        public static string ToPascalCase(this string str)
        {
            if (string.IsNullOrEmpty(str)) return str;
            return char.ToUpper(str[0]) + str.Substring(1);
        }

        public static string Truncate(this string str, int maxLength)
        {
            if (string.IsNullOrEmpty(str) || str.Length <= maxLength)
                return str;
            return str.Substring(0, maxLength) + "...";
        }
    }

    // Abstract base class
    public abstract class BaseEntity
    {
        public string Id { get; init; } = Guid.NewGuid().ToString();
        public DateTime CreatedAt { get; init; } = DateTime.UtcNow;
        public DateTime? UpdatedAt { get; set; }
    }

    // Internal class
    internal class CacheManager<T>
    {
        private readonly Dictionary<string, T> _items = new();
        private readonly int _maxSize;

        public CacheManager(int maxSize = 100)
        {
            _maxSize = maxSize;
        }

        public void Set(string key, T value)
        {
            if (_items.Count >= _maxSize)
            {
                var oldest = _items.Keys.First();
                _items.Remove(oldest);
            }
            _items[key] = value;
        }

        public T? Get(string key)
        {
            return _items.TryGetValue(key, out var value) ? value : default;
        }
    }

    // Constants class
    public static class Constants
    {
        public const string Version = "1.0.0";
        public const int DefaultTimeout = 30;
        public const int MaxRetries = 3;
    }
}
