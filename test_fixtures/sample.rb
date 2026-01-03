=begin
@dose
purpose: Sample Ruby fixture for testing the luny Ruby parser.
    This file contains various Ruby constructs including classes, modules,
    methods, and metaprogramming patterns to verify extraction works correctly.

when-editing:
    - !Keep all export types represented for comprehensive testing
    - Maintain the mix of class and module patterns

invariants:
    - All public items must have clear, testable names
    - Include both instance and class methods

do-not:
    - Remove any exports without updating corresponding tests

gotchas:
    - Ruby private methods are declared after the `private` keyword
    - Module methods require explicit `module_function` or `self.` prefix
=end

require 'json'
require 'fileutils'
require_relative 'helper'

# Module with mixin functionality
module Loggable
  def log(message)
    puts "[#{Time.now.iso8601}] #{message}"
  end

  def log_error(message)
    log("ERROR: #{message}")
  end
end

# Module with class methods
module StringUtils
  module_function

  def camelize(str)
    str.split('_').map(&:capitalize).join
  end

  def underscore(str)
    str.gsub(/([A-Z])/, '_\1').downcase.sub(/^_/, '')
  end

  def truncate(str, length = 50)
    str.length > length ? "#{str[0...length]}..." : str
  end
end

# Configuration class
class UserConfig
  attr_accessor :id, :name, :email, :settings

  def initialize(id:, name:, email: nil, settings: {})
    @id = id
    @name = name
    @email = email
    @settings = settings
  end

  def to_h
    { id: @id, name: @name, email: @email, settings: @settings }
  end

  def to_json(*args)
    to_h.to_json(*args)
  end

  def self.from_json(json_str)
    data = JSON.parse(json_str, symbolize_names: true)
    new(**data)
  end
end

# Service class with inheritance
class BaseService
  include Loggable

  def initialize(data_dir = 'data')
    @data_dir = data_dir
    @cache = {}
  end

  def get(id)
    raise NotImplementedError, 'Subclasses must implement #get'
  end

  def save(item)
    raise NotImplementedError, 'Subclasses must implement #save'
  end

  protected

  def cache_get(id)
    @cache[id]
  end

  def cache_set(id, item)
    @cache[id] = item
  end
end

class UserService < BaseService
  def get(id)
    return cache_get(id) if cache_get(id)

    filepath = File.join(@data_dir, "#{id}.json")
    return nil unless File.exist?(filepath)

    json_data = File.read(filepath)
    user = UserConfig.from_json(json_data)
    cache_set(id, user)
    user
  rescue JSON::ParserError => e
    log_error("Failed to parse user file: #{e.message}")
    nil
  end

  def save(user)
    validate!(user)
    filepath = File.join(@data_dir, "#{user.id}.json")
    FileUtils.mkdir_p(@data_dir)
    File.write(filepath, user.to_json)
    cache_set(user.id, user)
    log("Saved user #{user.id}")
    true
  rescue IOError => e
    log_error("Failed to save user: #{e.message}")
    false
  end

  def all_users
    Dir.glob(File.join(@data_dir, '*.json')).map do |filepath|
      id = File.basename(filepath, '.json')
      get(id)
    end.compact
  end

  private

  def validate!(user)
    raise ArgumentError, 'User ID required' if user.id.nil? || user.id.empty?
    raise ArgumentError, 'User name required' if user.name.nil? || user.name.empty?
  end
end

# Factory module
module UserFactory
  def self.create(name:, email: nil)
    UserConfig.new(
      id: SecureRandom.uuid,
      name: name,
      email: email,
      settings: {}
    )
  end

  def self.create_admin(name:, email:)
    user = create(name: name, email: email)
    user.settings[:role] = 'admin'
    user
  end
end

# Constants
VERSION = '1.0.0'
DEFAULT_TIMEOUT = 30
MAX_RETRIES = 3
