# scripts/fetch-mcp-pack.ps1
$ErrorActionPreference = "Stop"

Write-Host "==> Récupération du pack MCP (Windows)..." -ForegroundColor Green

# Exemple de téléchargement et décompression en PowerShell
# Adapte les URL / chemins selon la logique de ton script .sh
$mcpUrl = "https://example.com/path/to/mcp-pack.zip"
$outputZip = "mcp-pack.zip"

Invoke-WebRequest -Uri $mcpUrl -OutFile $outputZip
Expand-Archive -Path $outputZip -DestinationPath "mcp-servers" -Force
Remove-Item $outputZip

Write-Host "==> Pack MCP récupéré avec succès !" -ForegroundColor Green