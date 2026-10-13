#!/usr/bin/env ruby
# Execute the store admission from the pinned published Action, not a local mirror.
require 'yaml'
require 'open3'
require 'pathname'

root = Pathname.new(ARGV.fetch(0) { File.expand_path('../..', __dir__) })
workflow = YAML.safe_load_file(root.join('.github/workflows/action-tests.yml'), aliases: true)
job = workflow.fetch('jobs').fetch('oidc_provider_detection')
abort 'store contract job must be unconditional' if job.key?('if') || job.key?('continue-on-error')
checks = job.fetch('steps').select { |step| step['name'] == 'Test published store admission' }
expected = {
  'name' => 'Test published store admission',
  'shell' => 'bash',
  'run' => 'ruby scripts/ci/test-action-store-contract.rb'
}
abort 'workflow must execute the published store contract test without overrides' unless checks == [expected]

action = YAML.safe_load_file(root.join('scripts/ci/fixtures/assay-action-pin/action.yml'), aliases: true)
steps = action.fetch('runs').fetch('steps')
guard = steps.fetch(0)
abort 'store admission must be the first unconditional Bash step' unless
  guard['id'] == 'store-scheme' && guard['shell'] == 'bash' &&
  !guard.key?('if') && !guard.key?('continue-on-error') &&
  steps.count { |step| step['id'] == 'store-scheme' } == 1
bindings = {
  'STORE' => '${{ inputs.store }}', 'PROVIDER' => '${{ inputs.store_provider }}',
  'AZURE_CLIENT_ID' => '${{ inputs.azure_client_id }}',
  'AZURE_TENANT_ID' => '${{ inputs.azure_tenant_id }}',
  'AZURE_SUBSCRIPTION_ID' => '${{ inputs.azure_subscription_id }}'
}
abort 'store admission input bindings changed' unless guard['env'] == bindings

cases = [
  ['', 'auto', true], ['s3://bucket/prefix', 'auto', true],
  ['s3://bucket/prefix', 'aws', true],
  ['gs://bucket/prefix', 'auto', false], ['az://container/prefix', 'auto', false],
  ['https://account.blob.core.windows.net/container', 'auto', false],
  ['file:///tmp/store', 'auto', false], ['memory://scratch', 'auto', false],
  ['https://example.com/store', 'auto', false],
  ['s3://bucket/prefix', 'gcp', false], ['s3://bucket/prefix', 'azure', false]
]
cases.each do |store, provider, accepted|
  env = bindings.transform_values { '' }.merge('PATH' => ENV.fetch('PATH'), 'STORE' => store, 'PROVIDER' => provider)
  stdout, stderr, result = Open3.capture3(env, 'bash', '-c', guard.fetch('run'), unsetenv_others: true)
  abort "unexpected admission for #{store.inspect}/#{provider}: #{result.exitstatus}: #{stdout}#{stderr}" unless
    result.exitstatus == (accepted ? 0 : 1) && (accepted || stdout.include?('::error::'))
end
puts "Published store admission: #{cases.length} cases passed; workflow invocation checked"
