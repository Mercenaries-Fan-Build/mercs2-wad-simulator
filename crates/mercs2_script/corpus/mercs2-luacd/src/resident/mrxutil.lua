import("MrxCheatBootstrap")
import("MrxGui")
import("MrxGuiShellBootstrap")
import("MrxState")
import("WifPmcInterior")
import("MrxHqManager")

function CallWithOptionalArgs(fFunction, tArgs)
  if type(fFunction) == "function" then
    if type(tArgs) == "table" then
      return fFunction(unpack(tArgs))
    else
      return fFunction()
    end
  end
end

function SetDefault(vVar, vDefaultValue)
  if vVar == nil then
    vVar = vDefaultValue
  end
  return vVar
end

function CopyTable(tSrc)
  local tDest = {}
  for k, v in pairs(tSrc) do
    if type(v) == "table" then
      tDest[k] = CopyTable(v)
    else
      tDest[k] = v
    end
  end
  return tDest
end

function MergeIndexedTables(...)
  local tReturnVal = {}
  for i, tTable in ipairs(arg) do
    if type(tTable) == "table" then
      for j, v in ipairs(tTable) do
        table.insert(tReturnVal, v)
      end
    end
  end
  return tReturnVal
end

function GetTableAsString(tInput, nTabs)
  nTabs = SetDefault(nTabs, 0)
  local sOutput = ""
  for k, v in pairs(tInput) do
    for i = 0, nTabs do
      sOutput = sOutput .. "\t"
    end
    if type(v) == "table" then
      sOutput = sOutput .. tostring(k) .. " = {\n" .. GetTableAsString(v, nTabs + 1)
      for i = 0, nTabs do
        sOutput = sOutput .. "\t"
      end
      sOutput = sOutput .. "}\n"
    else
      sOutput = sOutput .. tostring(k) .. " = " .. tostring(v) .. "\n"
    end
  end
  return sOutput
end

_tNumbers = false

function FormatMoney(n)
  local nFactor = 1000
  local sSuffix = "[0xe00c096a]"
  if 1.0E15 < n then
    n = 1.0E15
  end
  if n < 0 then
    n = 0
  end
  for nTestFactor, sTestSuffix in pairs(_tNumbers) do
    if nTestFactor > nFactor and nTestFactor <= n then
      nFactor = nTestFactor
      sSuffix = sTestSuffix
    end
  end
  local s = string.format("[SHELL.Common.Money:%d:%d:%s]", n / nFactor, 10 * (n / nFactor - math.floor(n / nFactor)), sSuffix)
  return s
end

function TeleportSecondaryHeroToPrimaryHero()
  local uPrimaryPlayer = Player.GetPrimaryPlayer()
  local uSecondaryPlayer = Player.GetSecondaryPlayer()
  local uPrimaryCharacter = Player.GetPrimaryCharacter()
  local uSecondaryCharacter = Player.GetSecondaryCharacter()
  local nYaw = Object.GetYaw(uPrimaryCharacter)
  local x, y, z = Object.GetPosition(uPrimaryCharacter)
  local uVehicle = Player.GetControlledObject(uSecondaryPlayer)
  if uVehicle and uVehicle ~= uSecondaryCharacter then
    Vehicle.Exit(uVehicle, uSecondaryCharacter)
  end
  if not Object.IsAlive(uSecondaryCharacter) then
    Object.Revive(uSecondaryCharacter, 0.25)
  end
  Object.DisablePhysics(uSecondaryCharacter)
  Object.SetYaw(uSecondaryCharacter, nYaw)
  local uPlayerCam = Player.GetCamera(uSecondaryPlayer)
  Camera.SetYaw(uPlayerCam, 0)
  Player.SetWaitForInGame(uSecondaryCharacter)
  Object.SetPosition(uSecondaryCharacter, x, y, z, false)
  if Player.TeleportCamera then
    Player.TeleportCamera(uSecondaryPlayer)
  end
  local bSuccess = Sys.RequestGameState("waitfortether")
  ASSERT(bSuccess, "Game state request unsuccessful")
  Event.Create(Event.GameStateChange, {
    "waitfortether",
    "Exit"
  }, function()
    Object.EnablePhysics(uSecondaryCharacter)
    local uVehicle = Vehicle.GetFromRider(uPrimaryCharacter)
    local bEnteredVehicle = EnterBestAvailableSeat(uSecondaryCharacter, uVehicle, 0, true)
    if bEnteredVehicle then
      Event.Create(Event.TimerRelative, {3, true}, function()
        Object.EnablePhysics(uSecondaryCharacter)
        Player.TeleportCamera(uSecondaryPlayer)
      end)
    end
  end)
end

function PlaceSecondaryPlayer()
  local bSuccess = Sys.RequestGameState("waitfortether")
  ASSERT(bSuccess, "Game state request unsuccessful")
  local uSecondaryCharacter = Player.GetSecondaryCharacter()
  Event.Create(Event.GameStateChange, {
    "waitfortether",
    "Exit"
  }, function()
  end)
end

function TeleportHeroesToLocations(tLocations, fCallback, tCallbackArgs, bEnterState, bExitState)
  if MrxCheatBootstrap.IsSkipModeEnabled() then
    CallWithOptionalArgs(fCallback, tCallbackArgs)
    return
  end
  bEnterState = SetDefault(bEnterState, true)
  bExitState = SetDefault(bExitState, true)
  local tPlayers = Player.GetAllPlayers()
  local tOperations = {}
  for i, uPlayer in ipairs(tPlayers) do
    local uHero = Player.GetCharacter(uPlayer)
    local vLocation
    if i <= table.getn(tLocations) then
      vLocation = tLocations[i]
    else
      vLocation = tLocations[1]
    end
    local nX, nY, nZ, nYaw
    local sType = type(vLocation)
    if 1 < i then
      local bIsValidLocation = sType == "string" or sType == "userdata" or sType == "table"
      if not bIsValidLocation then
        vLocation = tLocations[1]
        sType = type(vLocation)
      end
    end
    if sType == "string" or sType == "userdata" then
      if sType == "string" then
        vLocation = Pg.GetGuidByName(vLocation)
      end
      nX, nY, nZ = Object.GetPosition(vLocation)
      nYaw = Object.GetYaw(vLocation)
    elseif sType == "table" then
      nX = vLocation[1]
      nY = vLocation[2]
      nZ = vLocation[3]
      nYaw = vLocation[4] or 0
    end
    if not (uHero and nX and nY) or not nZ then
      Debug.Printf("Invalid Location ( " .. tostring(nX) .. ", " .. tostring(nY) .. ", " .. tostring(nZ) .. ") - " .. tostring(vLocation))
    else
      table.insert(tOperations, {
        uPlayer = uPlayer,
        uHero = uHero,
        nX = nX,
        nY = nY,
        nZ = nZ,
        nYaw = nYaw
      })
      if Net.IsServer() then
        Net.SetLastHeroTeleportLocation(nX, nY, nZ, nYaw)
        if not Player.IsLocal(uPlayer) then
          Player.SetWaitForInGame(uHero)
          if bEnterState == true then
            Net.SetLoadingScreen(true)
          end
          Net.SendEvent_TeleportPlayer(uPlayer, nX, nY, nZ, nYaw)
        end
      end
    end
  end
  _TeleportHeroes(tOperations, fCallback, tCallbackArgs, bEnterState, bExitState)
end

function TeleportHeroesToHardpoints(tHardpoints, fCallback, tCallbackArgs, bEnterState, bExitState)
  local tOperations = {}
  local tPlayers = Player.GetAllPlayers()
  for i, uPlayer in ipairs(tPlayers) do
    local uHero = Player.GetCharacter(uPlayer)
    local tHardpoint = tHardpoints[i]
    if tHardpoint then
      local uObject = tHardpoint.vObject
      if type(uObject) == "string" then
        uObject = Pg.GetGuidByName(uObject)
      end
      table.insert(tOperations, {
        uPlayer = uPlayer,
        uHero = uHero,
        uObject = uObject,
        sHardpoint = tHardpoint.sHardpoint
      })
    end
  end
  bEnterState = SetDefault(bEnterState, true)
  bExitState = SetDefault(bExitState, true)
  _TeleportHeroes(tOperations, fCallback, tCallbackArgs, bEnterState, bExitState)
end

function _TeleportHeroes(tOperations, fCallback, tCallbackArgs, bEnterState, bExitState)
  if table.getn(tOperations) <= 0 then
    return
  end
  if bEnterState then
    if Net.IsClient() then
      MrxState.SetQuickFade(false)
    end
    if Net.IsServer() then
      Net.SetLoadingScreen(true)
    end
    MrxState.Enter(MrxState.STATE_WAITFORSTREAMING, _CompleteTeleportHeroes, {
      tOperations,
      fCallback,
      tCallbackArgs,
      bEnterState,
      bExitState
    }, _TeleportStreamingComplete, {
      tOperations,
      fCallback,
      tCallbackArgs,
      bExitState
    })
  else
    _TeleportStreamingComplete(tOperations, fCallback, tCallbackArgs, bExitState)
    _CompleteTeleportHeroes(tOperations, fCallback, tCallbackArgs, bEnterState, bExitState)
  end
end

function _CompleteTeleportHeroes(tOperations, fCallback, tCallbackArgs, bEnterState, bExitState)
  if WifPmcInterior.IsInside() and not WifPmcInterior.IsEntering() then
    WifPmcInterior.Exit(-1, false)
  end
  for i, tOperation in ipairs(tOperations) do
    if Player.IsLocal(tOperation.uPlayer) then
      ExitAllPlayerModes(tOperation.uPlayer)
      if Human.IsCarrying(tOperation.uHero) then
        Human.Drop(tOperation.uHero, true)
      end
      if Human.IsGrappling(tOperation.uHero) then
        Human.StopGrappling(tOperation.uHero)
      end
      local uVehicle = Player.GetControlledObject(tOperation.uPlayer)
      if uVehicle and uVehicle ~= tOperation.uHero then
        Vehicle.Exit(uVehicle, tOperation.uHero, true)
        Event.Create(Event.Player, {
          tOperation.uPlayer,
          "Human",
          "Enter"
        }, _TeleportHero, {
          tOperations,
          i,
          bExitState,
          fCallback,
          tCallbackArgs
        })
      else
        local uExitingVehicle = Vehicle.GetFromRider(tOperation.uHero)
        if uExitingVehicle then
          Vehicle.Exit(uExitingVehicle, tOperation.uHero, true)
        end
        Event.Create(Event.TimerRelative, {0.1, true}, _TeleportHero, {
          tOperations,
          i,
          bExitState,
          fCallback,
          tCallbackArgs
        })
      end
    end
  end
end

function _TeleportStreamingComplete(tOperations, fCallback, tCallbackArgs, bExitState)
  tOperations.bStreamingComplete = true
  _TeleportComplete(tOperations, fCallback, tCallbackArgs, bExitState)
end

function _TeleportHero(tOperations, i, bExitState, fCallback, tCallbackArgs)
  local tOperation = tOperations[i]
  local bTeleportToLocation = (tOperation.nX and tOperation.nY and tOperation.nZ) ~= nil
  local bTeleportToHardpoint = (tOperation.uObject and tOperation.sHardpoint) ~= nil
  if bTeleportToLocation or bTeleportToHardpoint then
    Debug.Printf("Disabling physics for teleport")
    Human.SetState(Player.GetLocalCharacter(), "upright", "idle")
    Object.DisablePhysics(tOperation.uHero)
  end
  if tOperation.nYaw then
    Debug.Printf("Desired Yaw - " .. tostring(tOperation.nYaw))
    Object.SetYaw(tOperation.uHero, tOperation.nYaw)
  end
  if bTeleportToLocation or bTeleportToHardpoint then
    if not Object.IsAlive(tOperation.uHero) then
      Object.Revive(tOperation.uHero, 0.25)
    end
    Player.SetWaitForInGame(tOperation.uHero)
    if bTeleportToLocation then
      Debug.Printf("@@@@@@@@@@ MrxUtil._TeleportHero: Teleporting to " .. tOperation.nX .. "," .. tOperation.nY .. "," .. tOperation.nZ)
      Object.SetPosition(tOperation.uHero, tOperation.nX, tOperation.nY, tOperation.nZ, false)
    elseif bTeleportToHardpoint then
      Debug.Printf("@@@@@@@@@@ MrxUtil._TeleportHero: Teleporting to object " .. tostring(tOperation.uObject) .. ", hardpoint " .. tOperation.sHardpoint)
      local uObject = tOperation.uObject
      local sHardpoint = tOperation.sHardpoint
      Object.SetTransformToObject(tOperation.uHero, tOperation.uObject, tOperation.sHardpoint)
      if Net.IsServer() then
        local uPlayer = Player.GetSecondaryPlayer()
        if not uPlayer ~= 0 and not Player.IsLocal(uPlayer) then
          Debug.Printf("@@@@@@@@@@ MrxUtil._TeleportHero: Sending event to teleport to object " .. tostring(uObject) .. ", hardpoint " .. sHardpoint)
          Net.SendEvent_TeleportPlayerToHardPoint(uPlayer, uObject, sHardpoint)
        end
      end
    end
    Player.TeleportCamera(tOperation.uPlayer)
  end
  Human.PersistTransform(tOperation.uHero)
  tOperations[i].bComplete = true
  _TeleportComplete(tOperations, fCallback, tCallbackArgs, bExitState)
end

function _TeleportComplete(tOperations, fCallback, tCallbackArgs, bExitState)
  for i, tOperation in ipairs(tOperations) do
    Debug.Printf("@@@@@@@@@@ MrxUtil._TeleportComplete: tOperation[" .. i .. "].bComplete=" .. tostring(tOperation.bComplete))
  end
  Debug.Printf("@@@@@@@@@@ MrxUtil._TeleportComplete: tOperations.bStreamingComplete=" .. tostring(tOperations.bStreamingComplete))
  if not tOperations.bStreamingComplete then
    return
  end
  for _, tOperation in ipairs(tOperations) do
    if Player.IsLocal(tOperation.uPlayer) and not tOperation.bComplete then
      Debug.Printf("Teleport for " .. tostring(tOperation.uHero) .. " not complete....")
      return
    end
  end
  for _, tOperation in ipairs(tOperations) do
    Debug.Printf("Enabling physics for teleport")
    Object.EnablePhysics(tOperation.uHero)
    Human.SetState(tOperation.uHero, "Upright", "Idle")
    local uCamera = Player.GetCamera(tOperation.uPlayer)
    if uCamera then
      Camera.SetYaw(uCamera, 0)
      Camera.SetPitch(uCamera, 0.302)
    end
  end
  Event.Create(Event.TimerRelative, {0.75, true}, function()
    if bExitState then
      MrxState.Exit(MrxState.STATE_WAITFORSTREAMING)
    end
    CallWithOptionalArgs(fCallback, tCallbackArgs)
  end)
end

function EnterBestAvailableSeat(uCharacterGuid, uVehicleGuid, uAvoidSeatGuid, bImmediate)
  local bEnteredVehicle = false
  if uCharacterGuid and uVehicleGuid and Vehicle.EnterBySeatGuid and Vehicle.GetSeatByType then
    local sSeatName = "d"
    local uSeatGuid = Vehicle.GetSeatByType(uVehicleGuid, sSeatName, true)
    if not uSeatGuid or uSeatGuid == uAvoidSeatGuid then
      sSeatName = "g"
      uSeatGuid = Vehicle.GetSeatByType(uVehicleGuid, sSeatName, true)
    end
    if not uSeatGuid or uSeatGuid == uAvoidSeatGuid then
      sSeatName = "p"
      uSeatGuid = Vehicle.GetSeatByType(uVehicleGuid, sSeatName, true)
    end
    if not uSeatGuid or uSeatGuid == uAvoidSeatGuid then
      sSeatName = "c"
      uSeatGuid = Vehicle.GetSeatByType(uVehicleGuid, sSeatName, true)
    end
    if uSeatGuid and uSeatGuid ~= uAvoidSeatGuid then
      if sSeatName == "d" then
        Debug.Printf("entering driver's seat")
      elseif sSeatName == "g" then
        Debug.Printf("entering gunner's seat")
      elseif sSeatName == "p" then
        Debug.Printf("entering passenger's seat")
      elseif sSeatName == "c" then
        Debug.Printf("entering cargo seat")
      end
      bEnteredVehicle = Vehicle.EnterBySeatGuid(uVehicleGuid, uCharacterGuid, uSeatGuid, bImmediate)
    end
  end
  return bEnteredVehicle
end

function ProcessCallbackTable(tCallbacks, tAdditionalArgs)
  if type(tCallbacks) == "table" then
    for _, tCallback in ipairs(tCallbacks) do
      local tCallbackArgs = tCallback[2]
      local tArgs
      local bIsCallbackTable = type(tCallbackArgs) == "table"
      local bIsAdditionalTable = type(tAdditionalArgs) == "table"
      if bIsCallbackTable and bIsAdditionalTable then
        tArgs = MergeIndexedTables(tCallbackArgs, tAdditionalArgs)
      elseif bIsCallbackTable then
        tArgs = tCallbackArgs
      elseif bIsAdditionalTable then
        tArgs = tAdditionalArgs
      end
      CallWithOptionalArgs(tCallback[1], tArgs)
    end
  end
end

function FindSpawnPointOutOfView(iPathGuid, fRadius, iPrimaryPt)
  local iSecondaryPt
  iPrimaryPt = iPrimaryPt or Player.GetPrimaryCharacter()
  if iPrimaryPt == Player.GetPrimaryCharacter() then
    iSecondaryPt = Player.GetSecondaryCharacter()
  else
    iSecondaryPt = Player.GetPrimaryCharacter()
  end
  if iSecondaryPt == nil then
    iSecondaryPt = 0
  end
  return Pg.GetDistantSpawnPointOnPath(iPathGuid, iPrimaryPt, iSecondaryPt, fRadius)
end

function SpawnObject(sTemplate, sLocation, sName)
  if type(sLocation) == "string" then
    sLocation = Pg.GetGuidByName(sLocation)
  end
  local res
  if sLocation then
    local x, y, z = Object.GetPosition(sLocation)
    local yaw = Object.GetYaw(sLocation)
    res = Pg.Spawn(sTemplate, x, y, z, yaw)
    if res and sName then
      Object.SetName(res, sName)
    end
  end
  return res
end

function SpawnActor(sTemplate, sName, vAnchorObject, sAnchorHardpoint, nYaw, bLink, bHighDetail, fCallback, tData)
  Debug.Printf("Spawning actor " .. sName .. " (" .. sTemplate .. ")")
  local uGuid = Pg.GetGuidByName(sName)
  if not uGuid then
    uGuid = Pg.Spawn(sTemplate, 0, 0, 0, 0, false, true)
    if not uGuid then
      return nil
    end
  end
  if sName then
    Object.SetName(uGuid, sName)
  end
  if type(vAnchorObject) == "string" then
    local uAnchor = Pg.GetGuidByName(vAnchorObject)
    if uAnchor then
      Debug.Printf("Anchor object specified: " .. tostring(vAnchorObject) .. ", hardpoint " .. tostring(sAnchorHardpoint))
      Object.SetTransformToObject(uGuid, uAnchor, sAnchorHardpoint)
      if bLink == true then
        Object.Attach(uAnchor, sAnchorHardpoint, uGuid)
      end
    end
  else
    Debug.Printf("Anchor position specified: " .. tostring(vAnchorObject[1]) .. ", " .. tostring(vAnchorObject[2]) .. ", " .. tostring(vAnchorObject[3]))
    Object.SetPosition(uGuid, vAnchorObject[1], vAnchorObject[2], vAnchorObject[3])
  end
  if type(nYaw) == "number" then
    Object.SetYaw(uGuid, nYaw)
  end
  local bInanimate = sName == "HqInterior"
  local nDetail
  Event.Create(Event.ObjectHibernation, {
    uGuid,
    "awake",
    nDetail
  }, _SpawnActorComplete, {
    uGuid,
    sName,
    bInanimate,
    fCallback,
    tData
  })
  return uGuid
end

function _SpawnActorComplete(uGuid, sName, bInanimate, fCallback, tData)
  Debug.Printf("Spawned actor " .. sName .. " is ready for use...")
  if not bInanimate then
    Ai.Enable(uGuid, false)
    Object.DisablePhysics(uGuid)
    Vehicle.EnableTurret(uGuid, "head", false)
  end
  if fCallback then
    CallWithOptionalArgs(fCallback, tData)
  end
end

function SetupLoadingCallback(self, fCallback, tCallbackData)
  self._nLoadPending = 0
  self._fLoadCallback = fCallback
  self._tLoadData = tCallbackData
end

function CleanupLoadingCallback(self)
  self._nLoadPending = nil
  self._fLoadCallback = nil
  self._tLoadData = nil
end

function LoadingCallback(self)
  if self._nLoadPending ~= nil then
    self._nLoadPending = self._nLoadPending - 1
    if self._nLoadPending > 0 then
      return
    end
  end
  local fCallback = self._fLoadCallback
  local tData = self._tLoadData
  self._nLoadPending = nil
  self._fLoadCallback = nil
  self._tLoadData = nil
  if fCallback then
    CallWithOptionalArgs(fCallback, tData)
  else
    Debug.Printf("No load callback!")
  end
end

function GetPrimaryCharacterName()
  local tMappings = {
    mattias = "Mattias",
    jennifer = "Jennifer",
    chris = "Chris"
  }
  local sSelectedCharacter = GetCharacterIdentity(Player.GetPrimaryCharacter())
  return tMappings[sSelectedCharacter]
end

function ExplodeMissionName(sMissionName)
  local sFaction = string.sub(sMissionName, 1, 3)
  local sMissionType = string.sub(sMissionName, 4, 6)
  local sMissionNum = string.sub(sMissionName, 7, 9)
  local bMissionType
  if sMissionType == "Con" then
    bMissionType = true
  elseif sMissionType == "Job" then
    bMissionType = false
  end
  local nMissionNum = tonumber(sMissionNum)
  return sFaction, bMissionType, nMissionNum
end

function GetDistanceToObject(uObjectA, nX, nY, nZ, bIgnoreY)
  if type(uObjectA) == "string" then
    uObjectA = Pg.GetGuidByName(uObjectA)
  end
  local nDistance = Object.GetDistanceFrom(uObjectA, nX, nY, nZ, bIgnoreY)
  if not nDistance then
    Debug.Printf("MrxUtil.GetDistanceToObject: Bad position returned")
    return
  end
  return nDistance
end

function GetDistanceBetween(uObjectA, uObjectB, bIgnoreY)
  if type(uObjectA) == "string" then
    uObjectA = Pg.GetGuidByName(uObjectA)
  end
  if type(uObjectB) == "string" then
    uObjectB = Pg.GetGuidByName(uObjectB)
  end
  local nDistance = Object.GetDistanceFrom(uObjectA, uObjectB, bIgnoreY)
  if not nDistance then
    Debug.Printf("MrxUtil.GetDistanceBetween: Bad position returned")
    return
  end
  return nDistance
end

function TestDistanceToAllPlayers(uObject, nDistance, bIgnoreY)
  if type(uObject) == "string" then
    uObject = Pg.GetGuidByName(uObject)
  end
  if not uObject or not nDistance then
    Debug.Printf([[
@Name:  MrxUtil.TestDistanceToAllPlayers
@In  :  vObjectA: Object to test against
        nDistance: Distance to check
        bIgnoreY: if true, ignore Y when determining distance
@Out :  True if no players are within nDistance meters, false otherwise
@Desc:  Checks to see if any player is within a specified distance of the object]])
    return
  end
  local tAllPlayers = Player.GetAllPlayers()
  for _, uPlayer in ipairs(tAllPlayers) do
    if nDistance > GetDistanceBetween(Player.GetCharacter(uPlayer), uObject, bIgnoreY) then
      return false
    end
  end
  return true
end

function GetDistantLocations(tLocations, nDistance, bIgnoreY)
  if type(tLocations) ~= "table" or not nDistance then
    Debug.Printf([[
@Name:  MrxUtil.GetDistantLocations
@In  :  tLocations: table of objects to test
        nDistance: Distance to check
        bIgnoreY: if true, ignore Y when determining distance
@Out :  Table of objects that are nDistance or more way from all players
@Desc:  Takes a list of objects and removes objects that are within a specified distance of any
        player]])
    return
  end
  tGoodLocs = {}
  for _, loc in pairs(tLocations) do
    local uLocation = loc
    if type(loc) == "string" then
      uLocation = Pg.GetGuidByName(loc)
    end
    if TestDistanceToAllPlayers(uLocation, nDistance, bIgnoreY) then
      table.insert(tGoodLocs, loc)
    end
  end
  return tGoodLocs
end

function GetCharacterIdentity(uChar)
  local tLabels = {
    "mattias",
    "jennifer",
    "chris"
  }
  for i, label in pairs(tLabels) do
    if Object.HasLabel(uChar, label) then
      return label
    end
  end
  Debug.Printf("ERROR: GetCharacterIdentity. Player character is not one of M/J/C")
  return tLabels[1]
end

function EnableHeroWeapons(bEnable)
  local uChar1 = Player.GetPrimaryCharacter()
  local uChar2 = Player.GetSecondaryCharacter()
  for _, uChar in ipairs({uChar1, uChar2}) do
    if uChar then
      if bEnable then
        Human.EnableWeapons(uChar)
        Player.SetAimMode(Player.GetLocalPlayer(), true)
      else
        Human.DisableWeapons(uChar)
        Player.SetAimMode(Player.GetLocalPlayer(), false)
      end
    end
  end
end

function GetRandomTableElement(t)
  local vElem
  local n = table.getn(t)
  if 0 < n then
    local i = Math.randi(1, n)
    vElem = t[i]
  end
  return vElem
end

function GetFaction(uGuid)
  local tFactions = {
    "VZ",
    "Allied",
    "China",
    "Guerilla",
    "OC",
    "Pirate",
    "PMC",
    "Civ"
  }
  for i, faction in pairs(tFactions) do
    if Object.HasLabel(uGuid, faction) then
      return faction
    end
  end
end

function SetBoundaryEffect(bOutBoundary, fOpacity)
  local oSatellite = MrxGui.GetWidgetByNameAndOwner("Satellite overlay", Player.GetLocalPlayer())
  if oSatellite and oSatellite.CustomData.bActivated then
    return
  end
  Graphics.SetBoundaryEffect(fOpacity)
end

function DisplayHealthBar(self, uGuid, nOldHealth, bOptional, nOffset)
  tHealthBar = tHealthBar or {}
  tHealthBar[uGuid] = tHealthBar[uGuid] or {}
  if type(uGuid) == "string" then
    uGuid = Pg.GetGuidByName(uGuid)
  end
  if nOffset then
    nBarSlot = 2
  else
    nBarSlot = 1
  end
  Debug.Printf("DisplayHealthBar " .. tostring(uGuid) .. "(" .. tostring(Object.GetHealth(uGuid)) .. " health)")
  local nMaxHealth = Object.GetMaxHealth(uGuid)
  local nHealth = Object.GetHealth(uGuid)
  local nPercent = math.floor(nHealth / nMaxHealth * 100)
  local sColor = "[green]"
  if nPercent < 35 then
    sColor = "[red]"
  elseif nPercent < 75 then
    sColor = "[yellow]"
  end
  sHudText = Object.GetLocalizedName(uGuid) .. ":" .. sColor .. "[bar" .. nPercent .. "]"
  Hud.ObjectiveTray:SetSlotToText({nSlot = nBarSlot, sText = sHudText})
  if 0 < nHealth then
    tHealthBar[uGuid].HealthEvent = self:_CreateEvent(Event.ObjectHealth, {
      uGuid,
      "<",
      nHealth
    }, DisplayHealthBar, {
      self,
      uGuid,
      nHealth,
      bOptional,
      nOffset
    })
  end
  local sColor = "[yellow]"
  if bOptional then
    sColor = "[green]"
  end
  if nOldHealth and nOldHealth > nHealth then
    Hud.ObjectiveTray:SetSlotToText({
      nSlot = nBarSlot,
      sText = "[red]" .. Object.GetLocalizedName(uGuid) .. ":" .. sColor .. "[bar" .. nPercent .. "]"
    })
    tHealthBar[uGuid].TimerEvent = self:_CreateEvent(Event.TimerRelative, {0.35}, function()
      Hud.ObjectiveTray:SetSlotToText({
        nSlot = nBarSlot,
        sText = sColor .. Object.GetLocalizedName(uGuid) .. ":" .. sColor .. "[bar" .. nPercent .. "]"
      })
    end)
  else
    Hud.ObjectiveTray:SetSlotToText({
      nSlot = nBarSlot,
      sText = sColor .. Object.GetLocalizedName(uGuid) .. ":" .. sColor .. "[bar" .. nPercent .. "]"
    })
  end
end

function StopHealthBar(uGuid)
  if tHealthBar and tHealthBar[uGuid] then
    Event.Delete(tHealthBar[uGuid].HealthEvent)
    Event.Delete(tHealthBar[uGuid].TimerEvent)
    Hud.ObjectiveTray:SetSlotToText({nSlot = nBarSlot, sText = " "})
  else
    Debug.Printf([[
-------------------------------------------------
ERROR: No health bar events exist for that object!
-------------------------------------------------]])
  end
end

function GetPrimaryObjectiveRgb()
  return 255, 200, 0
end

function GetSecondaryObjectiveRgb()
  return 51, 204, 153
end

tInlineIcons = {
  "[objaction]",
  "[objaction2]",
  "[objoutpost]",
  "[objoutpost2]",
  "[objdeliver]",
  "[objdeliver2]",
  "[objdestroy]",
  "[objdestroy2]",
  "[objdefend]",
  "[objdefend2]",
  "[objverify]",
  "[objverify2]"
}

function GetInlineIconIndexByName(sName)
  for i, curIcon in ipairs(tInlineIcons) do
    if sName == curIcon then
      return i
    end
  end
end

function GetInlineIconNameByIndex(iIdx)
  return tInlineIcons[iIdx]
end

tObjWorldMarkers = {
  "global_objectivemarker",
  "MiniMap_Icon_Symbol_Yellow",
  "HUD_objective_action",
  "HUD_objective_outpost",
  "HUD_objective_defend",
  "HUD_objective_destroy",
  "HUD_objective_verify",
  "HUD_objective_deliverable",
  "HUD_objective_timer",
  "HUD_faction_GR",
  "HUD_faction_OC",
  "HUD_faction_PR",
  "HUD_faction_AN",
  "HUD_faction_CH",
  "HUD_Outpost_AN",
  "HUD_Outpost_CH",
  "HUD_Outpost_GR",
  "HUD_Outpost_OC",
  "HUD_Outpost_PR",
  "HUD_Outpost_AN_locked",
  "HUD_Outpost_CH_locked",
  "HUD_Outpost_GR_locked",
  "HUD_Outpost_OC_locked",
  "HUD_Outpost_PR_locked",
  "HUD_HQ_GR",
  "HUD_HQ_OC",
  "HUD_HQ_PMC",
  "HUD_HQ_CH",
  "HUD_HQ_AN",
  "HUD_HQ_AN_locked",
  "HUD_HQ_CH_locked",
  "HUD_HQ_GR_locked",
  "HUD_HQ_OC_locked",
  "pickup_fuel",
  "pickup_muntions",
  "HUD_PMC_Eva",
  "HUD_PMC_Ewan",
  "HUD_PMC_Fiona",
  "HUD_PMC_Misha",
  "HUD_exit",
  "HUD_wardrobe"
}
tObjPdaMarker = {
  "icon_yellow_mc",
  "icon_action_1_mc",
  "icon_action_2_mc",
  "icon_action_3_mc",
  "icon_outpost_1_mc",
  "icon_outpost_2_mc",
  "icon_outpost_3_mc",
  "icon_defend_1_mc",
  "icon_defend_2_mc",
  "icon_defend_3_mc",
  "icon_destroy_1_mc",
  "icon_destroy_2_mc",
  "icon_destroy_3_mc",
  "icon_verify_1_mc",
  "icon_verify_2_mc",
  "icon_verify_3_mc",
  "icon_deliverable_1_mc",
  "icon_deliverable_2_mc",
  "icon_deliverable_3_mc",
  "icon_pmc_mc",
  "icon_an_mc",
  "icon_ch_mc",
  "icon_gr_mc",
  "icon_oc_mc",
  "icon_pr_mc",
  "icon_vz_mc",
  "icon_an_locked_mc",
  "icon_ch_locked_mc",
  "icon_gr_locked_mc",
  "icon_oc_locked_mc",
  "icon_pr_locked_mc",
  "icon_vz_locked_mc",
  "icon_yellow_mc",
  ""
}
tObjRadarMaker = {
  "objective_destroy",
  "objective_deliverable",
  "objective_action",
  "objective_defend",
  "objective_verify",
  "objective_outpost",
  "temp_radar_icon_db",
  "temp_radar_icon_dbactive",
  "MiniMap_Icon_Faction_PMC",
  "MiniMap_Icon_Faction_GR",
  "MiniMap_Icon_Faction_OC",
  "MiniMap_Icon_Faction_PR",
  "MiniMap_Icon_Faction_AN",
  "MiniMap_Icon_Faction_CH",
  "MiniMap_Icon_Faction_VZ",
  "MiniMap_Icon_Faction_GR_locked",
  "MiniMap_Icon_Faction_OC_locked",
  "MiniMap_Icon_Faction_PR_locked",
  "MiniMap_Icon_Faction_AN_locked",
  "MiniMap_Icon_Faction_CH_locked",
  "MiniMap_Icon_Symbol_Yellow",
  "MiniMap_Icon_Misha",
  "MiniMap_Icon_Eva"
}

function _SearchMarkerTable(tTable, sName)
  for i, curName in ipairs(tTable) do
    if sName == curName then
      return i
    end
  end
  return 0
end

function MarkerGetNameByIndex_World(iIdx)
  return tObjWorldMarkers[iIdx]
end

function MarkerGetIndexByName_World(sName)
  local retVal = _SearchMarkerTable(tObjWorldMarkers, sName)
  if retVal == 0 and sName ~= "" then
    Debug.Printf("!!!!!!!!!!! CLIENT WON'T SEE THIS ... Could not find marker " .. sName .. " in World table")
  end
  return retVal
end

function MarkerGetNameByIndex_Pda(iIdx)
  return tObjPdaMarker[iIdx]
end

function MarkerGetIndexByName_Pda(sName)
  local retVal = _SearchMarkerTable(tObjPdaMarker, sName)
  if retVal == 0 and sName ~= "" then
    Debug.Printf("!!!!!!!!!!! CLIENT WON'T SEE THIS ... Could not find marker " .. sName .. " in Pda table")
  end
  return retVal
end

function MarkerGetNameByIndex_Radar(iIdx)
  return tObjRadarMaker[iIdx]
end

function MarkerGetIndexByName_Radar(sName)
  local retVal = _SearchMarkerTable(tObjRadarMaker, sName)
  if retVal == 0 and sName ~= "" then
    Debug.Printf("!!!!!!!!!!! CLIENT WON'T SEE THIS ... Could not find marker " .. sName .. " in Radar table")
  end
  return retVal
end

function ExitAllPlayerModes(uPlayer)
  Player.RequestPDAMapModeCancel(uPlayer)
  Player.SetCinematicMode(uPlayer, false)
  Player.SetSatelliteScanMode(uPlayer, false, 0, 0, 0)
end

function Init()
  _tNumbers = {}
  _tNumbers[1.0E15] = "[0x7be2637c]"
  _tNumbers[1.0E12] = "[0x9d96ba8f]"
  _tNumbers[1000000000] = "[0x4cf9c95f]"
  _tNumbers[1000000] = "[0xcd15e5e8]"
  _tNumbers[1000] = "[0xe00c096a]"
end

function ShieldFace(guid)
  local x, y, z = Object.GetPosition(guid)
  if x then
    for i, player in pairs(Player.GetAllPlayers()) do
      local hero = Player.GetCharacter(player)
      if GetDistanceToObject(hero, x, y, z) < 150 then
        Human.DoAction(hero, "shieldface")
      end
    end
  end
end

function GetNumberOfDigits(nNum)
  local nDigits = 0
  while 10 <= nNum do
    nNum = nNum / 10
    nDigits = nDigits + 1
  end
  return nDigits
end

function IsInside()
  return WifPmcInterior.IsInside() or MrxHqManager.IsInside()
end

function ClearVehiclesNearPoint(uPoint, uExceptVeh)
  if type(uPoint) == "string" then
    uPoint = Pg.GetGuidByName(uPoint)
  end
  local x, y, z = Object.GetPosition(uPoint)
  local t = Pg.FastCollectGroundVehicles(x, y, z, 5.5)
  Debug.Printf("-0- ground vehicles found: ", #t)
  for i, v in pairs(t) do
    if v ~= uExceptVeh then
      Object.Remove(v)
    end
  end
  t = Pg.FastCollectHelicopters(x, y, z, 12)
  Debug.Printf("-0- helicopters found: ", #t)
  for i, v in pairs(t) do
    if v ~= uExceptVeh then
      Object.Remove(v)
    end
  end
end
