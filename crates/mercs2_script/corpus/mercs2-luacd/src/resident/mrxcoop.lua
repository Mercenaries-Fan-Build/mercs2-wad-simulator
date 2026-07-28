import("MrxGui")
import("MrxPlayer")

function SetupTether(aTetherMin, aTetherMax)
  if _tEvents then
    DestroyTether()
  end
  _tEvents = {}
  iTetherMin = aTetherMin
  iTetherMax = aTetherMax
  Pg.SetBoundaryRadius(38.5)
end

function DestroyTether()
  for i, player in pairs(Player.GetAllPlayers()) do
    Player.SetOutBoundary(player, false)
  end
  for i, e in pairs(_tEvents) do
    Event.Delete(e)
  end
  _tEvents = nil
end

function AddPlayer(newPlayer)
  if not _primaryChar then
    local primaryPlayer = newPlayer
    _primaryChar = Player.GetCharacter(primaryPlayer)
  else
    local secondaryChar = Player.GetCharacter(newPlayer)
    local idx = tostring(secondaryChar)
    local sqDist = GetSquaredDistance(_primaryChar, secondaryChar)
    if sqDist < iTetherMin * iTetherMin then
      _TetherInsideMin(_primaryChar, secondaryChar)
    elseif sqDist > iTetherMax * iTetherMax then
      _TetherOutsideMax(_primaryChar, secondaryChar)
    else
      _TetherBetweenMinAndMax(_primaryChar, secondaryChar)
    end
  end
end

function GetRespawnOrigin()
  local x, y, z, yaw
  local tPlayers = Player.GetAllPlayers()
  for i, player in pairs(tPlayers) do
    local char = Player.GetCharacter(player)
    if char ~= nil and Object.IsAlive(char) then
      x, y, z = Object.GetPosition(char)
      yaw = Object.GetYaw(char)
      return x, y, z, yaw
    end
  end
  local respawnPoint = Pg.GetGuidByName("loc_playerStart")
  x, y, z = Object.GetPosition(respawnPoint)
  yaw = Object.GetYaw(respawnPoint)
  return x, y, z, yaw
end

function _TetherInsideMin(primary, secondary)
  local idx = tostring(secondary)
  local secondaryPlayer = Player.GetCharacter(secondary)
  PlayerAddMessage("INSIDE: " .. idx)
  local in_idx = idx .. "_in"
  _tEvents[in_idx] = nil
  local out_idx = idx .. "_out"
  Event.Delete(_tEvents[out_idx])
  _tEvents[out_idx] = nil
  local btw_idx = idx .. "_btw"
  _tEvents[btw_idx] = Event.Create(Event.ObjectProximity, {
    primary,
    secondary,
    ">=",
    iTetherMin,
    false,
    true
  }, _TetherBetweenMinAndMax, {primary, secondary})
end

function _TetherBetweenMinAndMax(primary, secondary)
  local idx = tostring(secondary)
  local secondaryPlayer = Player.GetCharacter(secondary)
  PlayerAddMessage(secondaryPlayer, "BETWEEN: " .. idx)
  local btw_idx = idx .. "_btw"
  _tEvents[btw_idx] = nil
  local in_idx = idx .. "_in"
  _tEvents[in_idx] = Event.Create(Event.ObjectProximity, {
    primary,
    secondary,
    "<",
    iTetherMin,
    false,
    true
  }, _TetherInsideMin, {primary, secondary})
  local out_idx = idx .. "_out"
  _tEvents[out_idx] = Event.Create(Event.ObjectProximity, {
    primary,
    secondary,
    ">=",
    iTetherMax,
    false,
    true
  }, _TetherOutsideMax, {primary, secondary})
  Player.SetOutBoundary(secondaryPlayer, false)
end

function _TetherOutsideMax(primary, secondary)
  local idx = tostring(secondary)
  local secondaryPlayer = Player.GetCharacter(secondary)
  PlayerAddMessage(secondaryPlayer, "OUTSIDE: " .. idx)
  local in_idx = idx .. "_in"
  Event.Delete(_tEvents[in_idx])
  _tEvents[in_idx] = nil
  local out_idx = idx .. "_out"
  _tEvents[out_idx] = nil
  local btw_idx = idx .. "_btw"
  _tEvents[btw_idx] = Event.Create(Event.ObjectProximity, {
    primary,
    secondary,
    "<",
    iTetherMax,
    false,
    true
  }, _TetherBetweenMinAndMax, {primary, secondary})
  Player.SetOutBoundary(secondaryPlayer, true)
end

function GetSquaredDistance(obj1, obj2)
  local x1, y1, z1 = Object.GetPosition(obj1)
  local x2, y2, z2 = Object.GetPosition(obj2)
  local dx = x1 - x2
  local dy = y1 - y2
  local dz = z1 - z2
  return dx * dx + dy * dy + dz * dz
end

function PlayerAddMessage(uPlayerGuid, sMsg)
  local msgBox = MrxGui.GetWidgetByNameAndOwner("MessageBox", uPlayerGuid)
  if msgBox then
    msgBox:AddMessage(sMsg)
  end
end
