import("MrxCoop")
import("MrxGui")
import("MrxGuiManager")
import("MrxGuiShellBootstrap")
import("MrxPlayState")
import("MrxPmc")
import("MrxUtil")
import("WifPmcInterior")
import("MrxMissionFlow")
import("MrxState")
import("MrxTransit")
import("MrxActionHijack")
import("MrxHqManager")
import("MrxVoSequence")
import("Hero")
import("MrxStatsManager")
import("Munitions")
import("MrxSound")
import("MrxGuiHudMessage")
local _tCharacterMap = {
  {
    base = "mattias",
    templates = {
      "mattiasupgrade1",
      "mattiasupgrade2",
      "mattiasupgrade3"
    },
    models = {
      "pmc_hum_mattias_v3",
      "pmc_hum_mattias_v2",
      "pmc_hum_mattias_v4",
      "pmc_hum_mattias_chickensuit",
      "gr_hum_starter_1",
      "pmc_hum_helipilot_unlockable",
      "pmc_hum_stealth",
      "pmc_hum_hoang",
      "pmc_hum_proppilot_unlockable",
      "gr_hum_boss_fake",
      "oc_hum_pilot"
    }
  },
  {
    base = "chris",
    templates = {
      "chrisupgrade1",
      "chrisupgrade2",
      "chrisupgrade3"
    },
    models = {
      "pmc_hum_chris_v2",
      "pmc_hum_chris_v3",
      "pmc_hum_chris_v4",
      "pmc_hum_chris_chickensuit",
      "pr_hum_starter05",
      "pmc_hum_blanco"
    }
  },
  {
    base = "jen",
    templates = {
      "jenupgrade1",
      "jenupgrade2",
      "jenupgrade3"
    },
    models = {
      "pmc_hum_jen_v3",
      "pmc_hum_jen_v5",
      "pmc_hum_jen_v2",
      "pmc_hum_jen_v4",
      "pmc_hum_jen_chickensuit",
      "pmc_hum_diablo",
      "pmc_hum_mechanic"
    }
  }
}
local tMsgMatClientJoin = {
  "Mattias.Misc.Joiner02",
  "Mattias.Misc.Joinee01",
  "Mattias.Misc.Joinee02",
  "Mattias.Misc.Joiner01"
}
local tMsgMatHostJoin = {
  "Mattias.Misc.Joinee01",
  "Mattias.Misc.Joinee02",
  "Mattias.Misc.Joiner01"
}
local tMsgJenClientJoin = {
  "Jen.Co_Op.Joiner01",
  "Jen.Co_Op.Joiner02"
}
local tMsgJenHostJoin = {
  "Jen.Co_Op.Joinee01"
}
local tMsgChrClientJoin = {
  "Chris.Misc.Joiner01",
  "Chris.Misc.Joiner02"
}
local tMsgChrHostJoin = {
  "Chris.Misc.Joinee01"
}
local tMsgJenHostLeft = {
  "Jen.Co_Op.Dropped01",
  "Jen.Co_Op.Dropped02"
}
local tMsgMatHostLeft = {
  "Mattias.Misc.Abandoned01",
  "Mattias.Misc.Abandoned02"
}
local tMsgChrHostLeft = {
  "Chris.Misc.Abandoned01",
  "Chris.Misc.Abandoned02"
}

function Init()
  for i = 0, Player.GetMaximumPlayers() - 1 do
    Debug.Printf("creating player " .. i)
    Player.CreatePlayer(i)
  end
end

function Deinit()
  MrxGuiManager.DeleteAllGuis()
  for i = 0, Player.GetMaximumPlayers() - 1 do
    Debug.Printf("destroying player " .. i)
    Player.DestroyPlayer(i)
  end
  Player.ClearPlayerDB()
end

function Start()
  Debug.Printf("registering player callbacks")
  Player.SetPlayerJoinedCallback(OnPlayerJoined)
  Player.SetPlayerLeftCallback(OnPlayerLeft)
end

function Reset()
  Debug.Printf("de-registering player callbacks")
  Player.RemovePlayerJoinedCallback()
  Player.RemovePlayerLeftCallback()
end

function SetLocalPlayerJoinedCallback(fFunc, tArgs)
  _fLocalPlayerJoinedFunc = fFunc
  _tLocalPlayerJoinedArgs = tArgs
  if Player.GetLocalPlayer() and _fLocalPlayerJoinedFunc ~= nil then
    MrxUtil.CallWithOptionalArgs(_fLocalPlayerJoinedFunc, _tLocalPlayerJoinedArgs)
  end
end

function SetSpawnLocations(tSpawnLocations)
  _tSpawnLocations = tSpawnLocations
  Debug.Printf("SetSpawnLocations " .. tostring(tSpawnLocations[1]))
end

function GetTemplateAndModelName(tCharacterConfig)
  if type(tCharacterConfig) ~= "table" then
    return tCharacterConfig
  end
  local iIndex = tCharacterConfig.iIndex
  if not iIndex then
    return
  end
  local tCharacterData = _tCharacterMap[iIndex]
  if not tCharacterData then
    return
  end
  local iUpgrade = tCharacterConfig.iUpgrade
  local tTemplates = tCharacterData.templates
  local sTemplate = iUpgrade and tTemplates and tTemplates[iUpgrade] or tCharacterData.base
  local iCostume = tCharacterConfig.iCostume
  local tModels = tCharacterData.models
  local sModel = iCostume and tModels and tModels[iCostume] or nil
  return sTemplate, sModel
end

function OnPlayerJoined(iPlayerId, sPlayerName, tCharacterConfig, bLocalPlayer, iLocalId)
  Debug.Printf("LUA: " .. sPlayerName .. " joined as player " .. iPlayerId .. ", and is local " .. tostring(bLocalPlayer))
  if bLocalPlayer then
    tCharacterConfig.iCostume = Player.GetProfileCostume()
  end
  local sTemplateName, sModelName = GetTemplateAndModelName(tCharacterConfig)
  sTemplateName = sTemplateName or GetSelectedCharacter(iPlayerId)
  Debug.Printf("LUA: " .. sPlayerName .. " joined as player " .. iPlayerId .. ", character " .. sTemplateName .. ", costume " .. tostring(sModelName))
  local vSpawnLocation = Player.GetPlayerStart()
  if _tSpawnLocations then
    vSpawnLocation = _tSpawnLocations[iPlayerId + 1]
  end
  local uCharacterGuid = CreatePlayerCharacter(bLocalPlayer, iPlayerId, sTemplateName, vSpawnLocation)
  if sModelName then
    Event.Create(Event.ObjectHibernation, {uCharacterGuid, "awake"}, Player.SetOutfit, {uCharacterGuid, sModelName})
  end
  if bLocalPlayer then
    Player.BindToLocal(iPlayerId, iLocalId)
  else
    Player.BindToRemote(iPlayerId)
  end
  if bLocalPlayer then
    local uPlayerGuid = Player.GetPlayer(iPlayerId)
    MrxGuiManager.CreateGui(uPlayerGuid)
  end
  Ai.AddSubject(uCharacterGuid)
  if bLocalPlayer and _fLocalPlayerJoinedFunc ~= nil then
    MrxUtil.CallWithOptionalArgs(_fLocalPlayerJoinedFunc, _fLocalPlayerJoinedArgs)
  end
  local uPlayerGuid = Player.GetPlayer(iPlayerId)
  local oCurrentMission = MrxPlayState.GetCurrentMission()
  if oCurrentMission then
    oCurrentMission:OnPlayerJoined(iPlayerId, uPlayerGuid, uCharacterGuid)
  end
  Event.Post("mpPlayerJoin", {uPlayerGuid, uCharacterGuid})
  Debug.Printf("LUA: Net.IsClient() - " .. tostring(Net.IsClient()) .. " bLocalPlayer - " .. tostring(bLocalPlayer))
  if Net.IsClient() and not bLocalPlayer then
    Event.Create(Event.GameStateChange, {
      "WaitForStreaming",
      "exit"
    }, Net.SendCustomEvent, {"MrxPlayer", NETEVENT_CLIENTDONESTREAMING}, true)
    if Player.GetFuelCapacity() > 300 then
      MrxPmc.SetFuelCapacity(Player.GetFuelCapacity())
    end
    if Player.GetFuel() > Player.GetFuelCapacity() then
      Player.SetFuel(Player.GetFuelCapacity())
    end
  end
  if Net.IsServer() and not bLocalPlayer then
    Munitions.OnPlayerJoined()
  end
end

function PlayJoinVO()
  if not Net.IsClient() then
    local sHostTemplateName = GetSelectedCharacter(0)
    local sClientTemplateName = GetSelectedCharacter(1)
    sClientVOString = nil
    sHostVOString = nil
    if sClientTemplateName == "mattias" then
      local iClientVO = math.randi(table.getn(tMsgMatClientJoin))
      sClientVOString = tMsgMatClientJoin[iClientVO]
    elseif sClientTemplateName == "jen" then
      if MrxMissionFlow._tActiveMissions.VzaCon001 == nil then
        table.insert(tMsgJenClientJoin, "Jen.Co_Op.Joinee02")
      end
      local iClientVO = math.randi(table.getn(tMsgJenClientJoin))
      sClientVOString = tMsgJenClientJoin[iClientVO]
    elseif sClientTemplateName == "chris" then
      if MrxMissionFlow._tActiveMissions.VzaCon001 == nil then
        table.insert(tMsgChrClientJoin, "Chris.Misc.Joinee02")
      end
      local iClientVO = math.randi(table.getn(tMsgChrClientJoin))
      sClientVOString = tMsgChrClientJoin[iClientVO]
    end
    if sHostTemplateName == "mattias" then
      local iHostVO = math.randi(table.getn(tMsgMatHostJoin))
      sHostVOString = tMsgMatHostJoin[iHostVO]
    elseif sHostTemplateName == "jen" then
      if MrxMissionFlow._tActiveMissions.VzaCon001 == nil then
        table.insert(tMsgJenHostJoin, "Jen.Co_Op.Joinee02")
      end
      local iHostVO = math.randi(table.getn(tMsgJenHostJoin))
      sHostVOString = tMsgJenHostJoin[iHostVO]
    elseif sHostTemplateName == "chris" then
      if MrxMissionFlow._tActiveMissions.VzaCon001 == nil then
        table.insert(tMsgChrHostJoin, "Chris.Misc.Joinee02")
      end
      local iHostVO = math.randi(table.getn(tMsgChrHostJoin))
      sHostVOString = tMsgChrHostJoin[iHostVO]
    end
    if sClientVOString == nil or sHostVOString == nil then
      return
    end
    Debug.Printf("LUA: sClientVOString - " .. tostring(sClientVOString) .. ", sHostVOString - " .. tostring(sHostVOString))
    MrxVoSequence.Start({
      {sClientVOString},
      {sHostVOString}
    })
  end
end

function PlayRandomLeftVO(tVO)
  local iVO = math.randi(table.getn(tVO))
  sVOString = tVO[iVO]
  MrxVoSequence.Start({
    {sVOString}
  })
end

function OnPlayerLeft(iPlayerId, sPlayerName, bLocalPlayer)
  if Player.GetLocalPlayerId() >= 0 and not MrxSound.ExitingGame() then
    sTemplateName = GetSelectedCharacter(Player.GetLocalPlayerId())
    if sTemplateName == "mattias" then
      PlayRandomLeftVO(tMsgMatHostLeft)
    elseif sTemplateName == "jen" then
      PlayRandomLeftVO(tMsgJenHostLeft)
    elseif sTemplateName == "chris" then
      PlayRandomLeftVO(tMsgChrHostLeft)
    end
  end
  Debug.Printf("LUA: " .. sPlayerName .. " left as player " .. iPlayerId)
  local uPlayerGuid = Player.GetPlayer(iPlayerId)
  local uCharacterGuid = Player.GetCharacter(uPlayerGuid)
  Event.Post("mpPlayerLeft", {uPlayerGuid, uCharacterGuid})
  local oCurrentMission = MrxPlayState.GetCurrentMission()
  if oCurrentMission then
    oCurrentMission:OnPlayerLeft(iPlayerId, uPlayerGuid, uCharacterGuid)
  end
  Vehicle.CancelHijack(uCharacterGuid)
  MrxGuiManager.DeleteGui(uPlayerGuid)
  Ai.RemoveSubject(uCharacterGuid)
  Player.Unbind(iPlayerId)
  DestroyPlayerCharacter(iPlayerId)
  Event.Delete(_evReviveNag)
  Event.Delete(_evCancelTimer)
  local bHeroesAlive = AreAnyHeroesAlive()
  if not bHeroesAlive and not MrxSound.ExitingGame() then
    if oCurrentMission then
      oCurrentMission:Cancel()
    elseif iPlayerId == 1 then
      PlayerDied(iPlayerId, uCharacterGuid)
    end
  end
end

function RequestPlayerRevive(iPlayerId, uChar, fX, fY, fZ)
  Event.Delete(_evCancelTimer)
  Event.Delete(_evReviveNag)
  Pg.RemoveContextAction(uChar)
  if Player.IsRemote(iPlayerId) then
    Net.SendEvent_RevivePlayer(iPlayerId)
  elseif uChar and not Object.IsAlive(uChar) then
    Object.Revive(uChar, 0.5)
    if fX and fY and fZ then
      Object.DisablePhysics(uChar)
      Object.SetPosition(uChar, fX, fY, fZ)
      Event.Create(Event.TimerRelative, {0.5}, function()
        Object.EnablePhysics(uChar)
      end)
    end
  end
end

function PlayerDied(iPlayerId, uChar)
  local oCurrentMission = MrxPlayState.GetCurrentMission()
  if Net.IsClient() and Player.IsBoundaryDeath(uChar) then
    Net.SendCustomEvent("MrxPlayer", NETEVENT_CLIENT_OUT_BOUNDARY_DEATH, {})
  end
  Event.Delete(_evCancelTimer)
  Event.Delete(_evReviveNag)
  if AreAnyHeroesAlive() then
    if not Object.IsAlive(uChar) and not Net.IsClient() then
      if Player.IsBoundaryDeath(uChar) then
        if oCurrentMission then
          oCurrentMission:Cancel()
        else
          MoveToSickbay()
        end
      else
        Pg.AddContextAction(uChar, "[ContextAction.Revive]", 2, 0, 0, 0, 0)
        Event.Create(Event.ContextAction, {
          Player.GetAnyCharacter(),
          uChar
        }, RequestPlayerRevive, {iPlayerId, uChar})
        VO.Cue(0, "Fiona.Misc.Revive01")
        if oCurrentMission then
          _evReviveNag = Event.Create(Event.TimerRelative, {30}, VO.Cue, {
            0,
            "Fiona.Misc.Revive02"
          })
        else
          _evReviveNag = Event.Create(Event.TimerRelative, {30}, VO.Cue, {
            0,
            "Fiona.Misc.Revive01"
          })
        end
        _evCancelTimer = Event.Create(Event.TimerRelative, {60}, function()
          if oCurrentMission then
            oCurrentMission:Cancel()
          else
            MoveToSickbay()
          end
        end)
      end
    end
  else
    if Net.IsClient() then
      return
    end
    if oCurrentMission then
      oCurrentMission:Cancel()
    else
      Event.Create(Event.TimerRelative, {2}, _DialogBoxDismissed, {1})
    end
    if Player.GetLocalPlayer() then
      Player.ClearGPS(Player.GetLocalPlayer())
    end
  end
  MrxStatsManager.IncreaseDeathCounter()
end

function _DisplayOnDeathDialogBox()
  if not AreAnyHeroesAlive() then
    MrxGui.DisplayDialogBox(Player.GetPrimaryPlayer(), "[Fanfare.Common.You_Have_Died]", {
      "[Fanfare.Common.Death_Continue]",
      "[Fanfare.Common.Quit]"
    }, 1, _DialogBoxDismissed, nil, nil, nil, nil, nil, nil, 2)
  end
end

function _DialogBoxDismissed(nIndex)
  if nIndex == 1 and string.lower(Sys.GetLevelName()) == "vz" then
    MoveToSickbay()
  else
  end
end

function CanMedEvac()
  local tPlayers = Player.GetAllPlayers()
  for i, uPlayerGuid in ipairs(tPlayers) do
    local uChar = Player.GetCharacter(uPlayerGuid)
    if not Object.IsAlive(uChar) or Player.InCinematicMode(uPlayerGuid) then
      return false
    end
  end
  if MrxTransit.IsInTransit() then
    return false
  end
  if MrxActionHijack.IsInHijack() then
    return false
  end
  if not WifPmcInterior.IsUnlocked() or WifPmcInterior.IsInside() then
    return false
  end
  if MrxHqManager.IsInside() then
    return false
  end
  if MrxState.IsLocked() then
    return false
  end
  if Net.IsClient() then
    return false
  end
  return true
end

function MedEvac()
  local oCurrentMission = MrxPlayState.GetCurrentMission()
  if oCurrentMission then
    oCurrentMission:SetCancelByMedEvac(true)
    oCurrentMission:Cancel()
    MrxVoSequence.Stop()
    VO.CancelAll()
  else
    MoveToSickbay()
  end
  Player.ClearGPS(Player.GetLocalPlayer())
  MrxStatsManager.IncreaseMedevacCounter()
end

function MoveToSickbay()
  if WifPmcInterior.IsInside() then
    return false
  end
  MrxVoSequence.Stop()
  VO.CancelAll()
  MrxState.Enter(MrxState.STATE_WAITFORGAME, _MoveToSickbayBegin)
  return true
end

function ResetLocalCharacter()
  local uCharGuid = Player.GetLocalCharacter()
  if uCharGuid then
    if Human.IsCarrying(uCharGuid) then
      Human.Drop(uCharGuid, true)
    end
    if not Object.IsAlive(uCharGuid) then
      ResetWeapons(uCharGuid)
    end
  end
end

function _MoveToSickbayBegin()
  local tPlayers = Player.GetAllPlayers()
  for i, uPlayerGuid in ipairs(tPlayers) do
    local uCharGuid = Player.GetCharacter(uPlayerGuid)
    if Human.IsGrappling(uCharGuid) then
      Human.StopGrappling(uCharGuid)
    end
    local uVehicle = Vehicle.GetFromRider(uCharGuid)
    if uVehicle then
      Vehicle.Exit(uVehicle, uCharGuid, true)
    end
  end
  ResetLocalCharacter()
  if Net.IsClient() then
    Net.SendCustomEvent("MrxPlayer", NETEVENT_CLIENTSELECTMEDEVAC, {})
    return
  end
  if Net.IsServer() then
    Net.SendCustomEvent("MrxPlayer", NETEVENT_HOST_SELECT_MEDEVAC, {})
  end
  WifPmcInterior.Enter(true, 2)
  Event.Create(Event.GameStateChange, {
    "WaitForStreaming",
    "exit"
  }, _CompleteMove)
  MrxState.Exit(MrxState.STATE_WAITFORGAME, _MoveToSickbayEnd)
end

function _CompleteMove()
  local tPlayers = Player.GetAllPlayers()
  for i, uPlayerGuid in ipairs(tPlayers) do
    local uCharGuid = Player.GetCharacter(uPlayerGuid)
    Object.SetTransformToObject(uCharGuid, "Sickbay Hero Respawn " .. i)
    local uCamera = Player.GetCamera(uPlayerGuid)
    Camera.SetYaw(uCamera, 80)
    Hero.EndSurvivalMode(uPlayerGuid, uCharGuid)
  end
  RiseFromYourGrave()
  Event.Post("MedevacComplete", tPlayers)
end

function _MoveToSickbayEnd()
  MrxPmc.AddCashQty(-GetMedEvacCost(), true, "[Generic.Medevacs]")
end

function GetMedEvacCost()
  return 10000
end

function ResetWeapons(uCharGuid, sNewWeapon)
  local sPrimary = sNewWeapon or "Pistol"
  local sGrenade = "Grenade"
  local sC4 = "C4"
  local uPrimary = Pg.GetGuidByName(sPrimary)
  local uGrenade = Pg.GetGuidByName(sGrenade)
  local uC4 = Pg.GetGuidByName(sC4)
  Human.Inventory.SetAllWeapons(uCharGuid, {
    uPrimary,
    uGrenade,
    uC4
  })
end

function RiseFromYourGrave()
  local tPlayers = Player.GetAllPlayers()
  for _, uPlayer in ipairs(tPlayers) do
    local uCharacter = Player.GetCharacter(uPlayer)
    if uCharacter == Player.GetPrimaryCharacter() and not Object.IsAlive(uCharacter) then
      Object.Revive(uCharacter)
    elseif uCharacter == Player.GetSecondaryCharacter() and not Object.IsAlive(uCharacter) then
      Net.SendEvent_RevivePlayer(1)
    end
  end
end

function OnPlayerInit(iPlayerId, uChar)
  if Object.IsAlive(uChar) then
    Event.CreatePersistent(Event.ObjectDeath, {uChar}, PlayerDied, {iPlayerId, uChar})
  else
    Event.Create(Event.TimerRelative, {2}, OnPlayerInit, {iPlayerId, uChar})
  end
end

function CreatePlayerCharacter(bLocalPlayer, iPlayerId, sCharacterName, vLocation)
  Debug.Printf("@@@@@@@@@@ MrxPlayer.CreatePlayerCharacter: id = " .. iPlayerId .. ": type = " .. tostring(sCharacterName) .. ": model = " .. tostring(sModelName) .. ": location = " .. tostring(vLocation))
  local x, y, z, yaw = 0, 0, 0, 0
  if not bLocalPlayer then
    Debug.Printf("Creating remote player - Setting location to local player")
    local uLocation = Player.GetLocalCharacter()
    if uLocation then
      x, y, z = Object.GetPosition(uLocation)
      yaw = Object.GetYaw(uLocation)
    end
  elseif vLocation then
    if type(vLocation) == "string" then
      local uLocation = Pg.GetGuidByName(vLocation)
      if uLocation then
        x, y, z = Object.GetPosition(uLocation)
        yaw = Object.GetYaw(uLocation)
      end
    elseif type(vLocation) == "table" then
      x = vLocation[1]
      y = vLocation[2]
      z = vLocation[3]
      yaw = vLocation[4]
    end
  end
  local uCharacterGuid = Pg.Spawn(sCharacterName, x, y, z, yaw, false, false, false)
  Player.AttachToCharacter(iPlayerId, uCharacterGuid)
  OnPlayerInit(iPlayerId, uCharacterGuid)
  return uCharacterGuid
end

function DestroyPlayerCharacter(iPlayerId)
  Debug.Printf("destroy player character: id = " .. iPlayerId)
  local uPlayerGuid = Player.GetPlayer(iPlayerId)
  if uPlayerGuid == nil then
    return
  end
  local uCharacterGuid = Player.GetCharacter(uPlayerGuid)
  if uCharacterGuid then
    Player.DetachFromCharacter(iPlayerId)
    Object.Remove(uCharacterGuid)
  end
end

function ChangePlayerCharacter(iPlayerId, sCharacterName, sCharacterModel)
  Debug.Printf("change player character: id = " .. iPlayerId .. ": type = " .. tostring(sCharacterName) .. ": model = " .. tostring(sModelName))
  local uPlayerGuid = Player.GetPlayer(iPlayerId)
  if uPlayerGuid == nil then
    return
  end
  local x, y, z, yaw
  local uCharacterGuid = Player.GetCharacter(uPlayerGuid)
  if uCharacterGuid then
    x, y, z = Object.GetPosition(uCharacterGuid)
    yaw = Object.GetYaw(uCharacterGuid)
    Player.DetachFromCharacter(iPlayerId)
    Object.Remove(uCharacterGuid)
  else
    x, y, z, yaw = 0, 0, 0, 0
  end
  uCharacterGuid = Pg.Spawn(sCharacterName, x, y, z, yaw, false, false, false)
  if sModelName then
    Object.SetModelName(uCharacterGuid, sModelName)
  end
  Player.AttachToCharacter(iPlayerId, uCharacterGuid)
  OnPlayerInit(iPlayerId, uCharacterGuid)
  return uCharacterGuid
end

function GetSelectedCharacter(iPlayerId)
  Debug.Printf("GetSelectedCharacter: iPlayerId = " .. iPlayerId)
  local uPlayerGuid = Player.GetPlayer(iPlayerId)
  Debug.Printf("uPlayerGuid = " .. tostring(uPlayerGuid))
  local uCharacterGuid = Player.GetCharacter(uPlayerGuid)
  if uCharacterGuid == nil then
    return MrxGuiShellBootstrap.GetSelectedCharacter() or Sys.GetCharacterTemplate() or "mattias"
  end
  Debug.Printf("uCharacterGuid = " .. tostring(uCharacterGuid))
  local uCharacterTemplate = Object.GetParent(uCharacterGuid)
  Debug.Printf("uCharacterTemplate = " .. tostring(uCharacterTemplate))
  if Object.HasLabel(uCharacterTemplate, "Mattias") then
    return "mattias"
  elseif Object.HasLabel(uCharacterTemplate, "Jennifer") then
    return "jen"
  elseif Object.HasLabel(uCharacterTemplate, "Chris") then
    return "chris"
  end
end

function AreAnyHeroesAlive()
  local tPlayers = Player.GetAllPlayers()
  for _, uPlayer in ipairs(tPlayers) do
    local uCharacter = Player.GetCharacter(uPlayer)
    if uCharacter and Object.IsAlive(uCharacter) then
      return true
    end
  end
  return false
end

function SaveSingleton()
  local tSaveData = {}
  local tPlayers = Player.GetAllPlayers()
  for i, uPlayerGuid in ipairs(tPlayers) do
    local uCharGuid = Player.GetCharacter(uPlayerGuid)
    local tEquipment = Human.Inventory.GetAllWeapons(uCharGuid, true)
    local tSavedEquipment = {}
    for i, uEquipment in pairs(tEquipment) do
      tSavedEquipment[i] = {
        Object.GetParent(uEquipment),
        Weapon.GetReserveAmmo(uEquipment)
      }
      Debug.Printf("Equipment " .. i .. " - Parent " .. tostring(tSavedEquipment[i][1]) .. " Reserve " .. tostring(tSavedEquipment[i][2]))
    end
    tSaveData[i] = {
      nHealth = Object.GetMaxHealth(uCharGuid),
      tEquipment = tSavedEquipment
    }
  end
  return tSaveData
end

function LoadSingleton(tSaveData)
  if not tSaveData then
    return
  end
  RiseFromYourGrave()
  local tPlayers = Player.GetAllPlayers()
  for i, uPlayerGuid in ipairs(tPlayers) do
    local uCharGuid = Player.GetCharacter(uPlayerGuid)
    local tCharData = tSaveData[i]
    if tCharData then
      if tCharData.tEquipment then
        function _RestoreEquipment(uGuid, tSavedEquipment)
          local tEquipment = {}
          
          for i, tEquipmentData in pairs(tSavedEquipment) do
            tEquipment[i] = tEquipmentData[1]
            Debug.Printf("Equipment " .. i .. " - Parent " .. tostring(tEquipment[i]))
          end
          Human.Inventory.SetAllWeapons(uGuid, tEquipment)
          local tNewEquipment = Human.Inventory.GetAllWeapons(uGuid, true)
          for i, uEquipment in pairs(tNewEquipment) do
            Debug.Printf("New Equipment " .. i .. " - Parent " .. tostring(Object.GetParent(uEquipment)) .. " Guid " .. tostring(uEquipment))
            if tSavedEquipment[i] then
              Event.Create(Event.ObjectHibernation, {uEquipment, "a"}, Weapon.SetReserveAmmo, {
                uEquipment,
                tSavedEquipment[i][2]
              })
            else
              Debug.Printf("@@@@@@@@@@ MrxPlayer.LoadSingleton: new equipment item at index " .. i .. " did not have a corresponding equipment item in the save data!")
            end
          end
        end
        
        Event.Create(Event.ObjectHibernation, {uCharGuid, "a"}, _RestoreEquipment, {
          uCharGuid,
          tCharData.tEquipment
        })
      end
      Object.SetHealth(uCharGuid, tCharData.nHealth)
    end
  end
end

function IsInVehicle(sFilter)
  local tPlayers = Player.GetAllPlayers()
  local oFilter = ObjectFilter.Create()
  local iInVehicle = 0
  sFilter = sFilter or "Vehicle"
  oFilter:SetFilter(sFilter)
  for i, uPlayerGuid in pairs(tPlayers) do
    local uCharGuid = Player.GetCharacter(uPlayerGuid)
    local uVehicleGuid = Vehicle.GetFromRider(uCharGuid)
    if uVehicleGuid and oFilter:Eval(uVehicleGuid) then
      iInVehicle = iInVehicle + 1
    end
  end
  if 0 < iInVehicle then
    return iInVehicle
  else
    return false
  end
end

function SetCharacterMap(tNewCharacterMap)
  if tNewCharacterMap == nil then
    return false
  end
  _tCharacterMap = nil
  _tCharacterMap = tNewCharacterMap
  return true
end

function KilledByPlayer(uGuid)
  Net.SendCustomEvent("MrxPlayer", NETEVENT_CLIENTKILL, {uGuid})
end

NETEVENT_CLIENTSELECTMEDEVAC = 0
NETEVENT_CLIENTDONESTREAMING = 1
NETEVENT_HOST_SELECT_MEDEVAC = 2
NETEVENT_CLIENTKILL = 3
NETEVENT_CLIENT_OUT_BOUNDARY_DEATH = 4

function NetEventCallback(nEventType, tArgs)
  if nEventType == NETEVENT_CLIENTSELECTMEDEVAC then
    Debug.Printf("Received ClientSelectedMedivac message")
    ResetLocalCharacter()
    WifPmcInterior.Enter(true, 2)
    Event.Create(Event.GameStateChange, {
      "WaitForStreaming",
      "exit"
    }, _CompleteMove)
  elseif nEventType == NETEVENT_HOST_SELECT_MEDEVAC then
    Debug.Printf("Received HostSelectedMedivac message")
    ResetLocalCharacter()
  elseif nEventType == NETEVENT_CLIENTDONESTREAMING then
    Event.Create(Event.TimerRelative, {4}, function()
      local uGuid = Player.GetLocalCharacter()
      if uGuid and not Object.IsAlive(uGuid) then
        Object.Revive(uGuid)
      end
      PlayJoinVO()
      MrxGuiHudMessage.OnPlayerJoined()
    end)
  elseif nEventType == NETEVENT_CLIENTKILL then
    if tArgs[1] then
      Event.Post("ClientKill", {
        tArgs[1]
      })
    end
  elseif nEventType == NETEVENT_CLIENT_OUT_BOUNDARY_DEATH and not Net.IsClient() then
    local oCurrentMission = MrxPlayState.GetCurrentMission()
    Event.Delete(_evCancelTimer)
    Event.Delete(_evReviveNag)
    if oCurrentMission then
      oCurrentMission:Cancel()
    else
      MoveToSickbay()
    end
  end
end
