import("MrxGui")
import("MrxCheatBootstrap")
import("MrxFactionManager")
import("MrxGuiBootstrap")
import("MrxLayerManager")
import("MrxMultiPageMenu")
import("MrxPlayer")
import("MrxPlayState")
import("MrxPmc")
import("MrxSound")
import("MrxStarterManager")
import("MrxState")
import("MrxUtil")
import("WifFreePlay")
import("WifMissionData")
import("WifMissionFlow")
import("WifPmcGarage")
import("WifVzBoundary")
import("MrxHq")
import("WifBriefingData")
import("MrxTransit")
import("MrxVoSequence")
import("MrxRewardData")
import("MrxGuiInterface")
_tStarters = {
  PmcBoss = {
    sSourceObject = "HqInterior",
    sBriefingLoc = "PmcInterior_StarterFiona_BriefingLoc",
    sHero1BriefingLoc = "PmcInterior_StarterFiona_BriefingLoc_Hero1",
    sWldBlpTexture = "HUD_PMC_Fiona"
  },
  HelPmcBoss = {
    sSourceObject = "_pmcoutpost_interior_recruitheli 0x000c73ec",
    sLayerName = "Vz_State_PmcInterior_Hel",
    sBriefingLoc = "PmcInterior_StarterEwen_BriefingLoc",
    sHero1BriefingLoc = "PmcInterior_StarterEwen_BriefingLoc_Hero1",
    sWldBlpTexture = "HUD_PMC_Ewan"
  },
  MecPmcBoss = {
    sSourceObject = "_pmcoutpost_interior_recruitmechanic 0x000c73ee",
    sLayerName = "Vz_State_PmcInterior_Mec",
    sAbsentLayerName = "Vz_State_PmcInterior_MecAbsent",
    sBriefingLoc = "PmcInterior_StarterEva_BriefingLoc",
    sHero1BriefingLoc = "PmcInterior_StarterEva_BriefingLoc_Hero1",
    sWldBlpTexture = "HUD_PMC_Eva"
  },
  JetPmcBoss = {
    sSourceObject = "_pmcoutpost_interior_recruitjet 0x000c740d",
    sLayerName = "Vz_State_PmcInterior_Jet",
    sBriefingLoc = "PmcInterior_StarterMisha_BriefingLoc",
    sHero1BriefingLoc = "PmcInterior_StarterMisha_BriefingLoc_Hero1",
    sWldBlpTexture = "HUD_PMC_Misha"
  }
}
_tPortals = {}
_tPortalData = {
  {
    sExterior_Entrance = "Starter_Pmc_Entrance",
    sExterior1 = "Starter_Pmc_Start1",
    sExterior2 = "Starter_Pmc_Start2",
    sInterior_Exit = "PmcInterior_A_Exit",
    sInterior1 = "PmcInterior_A1",
    sInterior2 = "PmcInterior_A2",
    sInteriorRoom = "MainHall"
  },
  {
    sExterior_Entrance = "Pmc_B_Entrance",
    sExterior1 = "Pmc_B1",
    sExterior2 = "Pmc_B2",
    sInterior_Exit = "PmcInterior_B_Exit",
    sInterior1 = "PmcInterior_B1",
    sInterior2 = "PmcInterior_B2",
    sInteriorRoom = "MainHall"
  },
  {
    sExterior_Entrance = "Pmc_C_Entrance",
    sExterior1 = "Pmc_C1",
    sExterior2 = "Pmc_C2",
    sInterior_Exit = "PmcInterior_C_Exit",
    sInterior1 = "PmcInterior_C1",
    sInterior2 = "PmcInterior_C2",
    sInteriorRoom = "MainHall"
  },
  {
    sExterior_Entrance = "Pmc_D_Entrance",
    sExterior1 = "Pmc_D1",
    sExterior2 = "Pmc_D2",
    sInterior_Exit = "PmcInterior_D_Exit",
    sInterior1 = "PmcInterior_D1",
    sInterior2 = "PmcInterior_D2",
    sInteriorRoom = "MainHall"
  }
}
_tInteriorPortalData = {}
_tAssetPreload = {
  animation = {
    Chris = {
      "player_chris_job_briefing_greeting",
      "player_chris_job_briefing_idle",
      "player_chris_job_briefing_no",
      "player_chris_job_briefing_yes",
      "player_chris_job_briefing_spiel"
    },
    Jennifer = {
      "player_jennifer_job_briefing_greeting",
      "player_jennifer_job_briefing_idle",
      "player_jennifer_job_briefing_no",
      "player_jennifer_job_briefing_yes",
      "player_jennifer_job_briefing_spiel"
    },
    Mattias = {
      "player_mattias_job_briefing_greeting_fb",
      "player_mattias_job_briefing_idle_fb",
      "player_mattias_job_briefing_no_fb",
      "player_mattias_job_briefing_yes_fb",
      "player_mattias_job_briefing_spiel_fb"
    },
    MaleStarter = {
      "all_starter02_job_briefing_idle",
      "all_starter02_job_briefing_greeting_neutral",
      "all_starter02_job_briefing_greeting_happy",
      "all_starter02_job_briefing_greeting_angry",
      "all_starter02_job_briefing_spiel",
      "all_starter02_job_briefing_goodbye"
    },
    FemaleStarter = {
      "all_starter03_job_briefing_idle",
      "all_starter03_job_briefing_greeting_neutral",
      "all_starter03_job_briefing_greeting_happy",
      "all_starter03_job_briefing_greeting_angry",
      "all_starter03_job_briefing_spiel",
      "all_starter03_job_briefing_goodbye"
    }
  },
  facefxanimationset = {
    Chris = {
      "Global_Job_Briefing_Chris"
    },
    Jennifer = {
      "Global_Job_Briefing_Jennifer"
    },
    Mattias = {
      "Global_Job_Briefing_Mattias"
    }
  },
  wavebank = {
    "vo_job_heros",
    "vo_job_pmc"
  },
  soundbank = {
    "vo_job_heros",
    "vo_job_pmc"
  }
}
_tOutfits = {
  chris = {
    {
      Name = "Original",
      Model = "pmc_hum_chris",
      PlayerVisibleName = "[SHELL.Misc.41]"
    },
    {
      Name = "Vacation",
      Model = "pmc_hum_chris_v2",
      PlayerVisibleName = "[SHELL.Misc.49]"
    },
    {
      Name = "Commando",
      Model = "pmc_hum_chris_v3",
      PlayerVisibleName = "[SHELL.Misc.50]"
    },
    {
      Name = "OffDuty",
      Model = "pmc_hum_chris_v4",
      PlayerVisibleName = "[SHELL.Misc.51]"
    },
    {
      Name = "ChickenSuit",
      Model = "pmc_hum_chris_chickensuit",
      PlayerVisibleName = "[SHELL.Misc.45]"
    }
  },
  jennifer = {
    {
      Name = "Original",
      Model = "pmc_hum_jen",
      PlayerVisibleName = "[SHELL.Misc.41]"
    },
    {
      Name = "Rebel",
      Model = "pmc_hum_jen_v3",
      PlayerVisibleName = "[SHELL.Misc.46]"
    },
    {
      Name = "Tactical",
      Model = "pmc_hum_jen_v5",
      PlayerVisibleName = "[SHELL.Misc.54]"
    },
    {
      Name = "NoJacket",
      Model = "pmc_hum_jen_v2",
      PlayerVisibleName = "[SHELL.Misc.47]"
    },
    {
      Name = "CatSuit",
      Model = "pmc_hum_jen_v4",
      PlayerVisibleName = "[SHELL.Misc.48]"
    },
    {
      Name = "ChickenSuit",
      Model = "pmc_hum_jen_chickensuit",
      PlayerVisibleName = "[SHELL.Misc.45]"
    }
  },
  mattias = {
    {
      Name = "Original",
      Model = "pmc_hum_mattias",
      PlayerVisibleName = "[SHELL.Misc.41]"
    },
    {
      Name = "MetalHead",
      Model = "pmc_hum_mattias_v3",
      PlayerVisibleName = "[SHELL.Misc.43]"
    },
    {
      Name = "Suit",
      Model = "pmc_hum_mattias_v2",
      PlayerVisibleName = "[SHELL.Misc.42]"
    },
    {
      Name = "Jacket",
      Model = "pmc_hum_mattias_v4",
      PlayerVisibleName = "[SHELL.Misc.44]"
    },
    {
      Name = "ChickenSuit",
      Model = "pmc_hum_mattias_chickensuit",
      PlayerVisibleName = "[SHELL.Misc.45]"
    }
  }
}
_tStockpileQty = {}
_tStockpile = {
  money = {
    1000,
    976562,
    1953125,
    3906250,
    7812500,
    15625000,
    31250000,
    625000000,
    125000000,
    250000000,
    500000000,
    1000000000
  },
  artillery = {
    1,
    5,
    9
  },
  bombingrun = {
    1,
    5,
    9
  },
  bunkerbuster = {
    1,
    2,
    4
  },
  clusterbomb = {
    1,
    4,
    7
  },
  combatairpatrol = {
    1,
    3,
    5
  },
  fuelairbomb = {
    1,
    2,
    3
  },
  laserguidedbomb = {
    1,
    4,
    7
  },
  moab = {
    1,
    2,
    2
  },
  rocketartillery = {
    1,
    2,
    3
  },
  surgicalstrike = {
    1,
    4,
    7
  },
  tankbuster = {
    1,
    2,
    4
  },
  daisycutter = {
    1,
    2,
    4
  }
}
_tBuildings = {
  "_pmcoutpost_bld_hq_livedin 0x000d3c77",
  "_pmcoutpost_bld_hqgarage_livedin 0x000d3c78",
  "_pmcoutpost_bld_hqsuites 0x000cf8c2"
}
_tBuildingEvents = {}
_tBuildingStates = {}
_bTeleport = true

function Unlock()
  _bUnlocked = true
  RefreshUiDisplay()
  WifPmcGarage.Unlock()
  for nBuilding, sBuildingName in ipairs(_tBuildings) do
    _OnPmcHibernation(nBuilding, true)
  end
end

function IsUnlocked()
  return _bUnlocked
end

function IsGarageAlive()
  return _tBuildingStates[2]
end

function SetWagerStatus(sMissionId, bWin)
  _sWagerMissionId = sMissionId
  _bWagerWin = bWin
  _bWagerMissionComplete = true
  local tRewards = MrxRewardData.GetRewards(_sWagerMissionId)
  local nWagerValue = tRewards.nWagered
  if not _bWagerWin and nWagerValue then
    nWagerValue = -nWagerValue
  end
  if nWagerValue ~= nil then
    MrxPmc.AddCashQty(nWagerValue, nil, "[Generic.Wagers]", true)
  end
  Debug.Printf("@ Setting Wager Status: Mission: ", sMissionId, " Won: ", bWin, " Wager: ", tRewards.nWagered)
end

function GetWagerStatus()
  return _sWagerMissionId, _bWagerWin
end

function SetEntranceLock(bSet)
  _bEntranceLock = bSet
end

function Enter(bTeleport, nPortal)
  if Net.IsClient() then
    return
  end
  if Net.IsServer() then
    Net.SetLoadingScreen(true)
  end
  _bEntering = true
  if not MrxCheatBootstrap.IsSkipModeEnabled() then
    _bTeleport = bTeleport
    _OnEnter(nPortal or 1)
  else
    MrxUtil.CallWithOptionalArgs(_fTeleportCallback, _tTeleportCallbackArgs)
    MrxUtil.CallWithOptionalArgs(_fUnloadCallback, _tUnloadCallbackArgs)
  end
end

function _OnEnter(nPortal)
  if _bInside then
    return
  end
  _bInside = true
  _bEntering = true
  MrxFactionManager.DisableReporting(true)
  if Net.IsServer() then
    if Net.IsServer() then
      Net.SetLoadingScreen(true)
    end
    Net.SetBriefingInterior("WifPmcInterior")
    _evClientJoinedPMC = Event.CreatePersistent(Event.ScriptEvent, {
      "mpPlayerJoin",
      function(tData)
        return true
      end
    }, _OnPlayerJoined)
  end
  MrxState.Enter(MrxState.STATE_WAITFORGAME, _CompleteOnEnter, {nPortal})
end

function _CompleteOnEnter(nPortal)
  MrxHq.GlobalEnter(true)
  _bHeroTeleportComplete = nil
  _bAssetsLoaded = nil
  _nPortal = nPortal
  _SetupStarters()
  _SetPmcTransitLocation(false)
  MrxPlayer.RiseFromYourGrave()
  WifFreePlay.StopNag()
  if Sound.StopAndFlushAllSounds then
    Sound.StopAndFlushAllSounds()
  end
  _LoadInterior()
  dynamic_import("MrxBriefing", _BriefingModuleLoaded)
  RefreshUiDisplay()
  Graphics.Camera.SetNearFar(0, 0.3, 500, 0)
end

function _LoadInterior()
  _tLayers = _GetStarterLayers()
  MrxLayerManager.Add(_tLayers, _OnInteriorLoad, nil, nil, nil, true)
end

function _GetStarterLayers()
  local t = {
    "Vz_State_PmcInterior"
  }
  for sStarterId, tStarterData in pairs(_tStarters) do
    if tStarterData.oStarter then
      if tStarterData.sLayerName then
        table.insert(t, tStarterData.sLayerName)
      end
    elseif tStarterData.sAbsentLayerName then
      table.insert(t, tStarterData.sAbsentLayerName)
    end
  end
  return t
end

function _OnInteriorLoad()
  Graphics.Atmosphere.ChangeLineRegionSetting(Pg.GetGuidByName("rgn_atmo_interior"), "pmc")
  local tPortalData = _tPortalData[_nPortal]
  if _bTeleport then
    MrxUtil.TeleportHeroesToLocations({
      tPortalData.sInterior1,
      tPortalData.sInterior2
    }, _LoadStarters)
  else
    _bTeleport = true
    _LoadStarters()
  end
  _sCurrentRoom = _tPortalData[_nPortal].sInteriorRoom
  _EnableInteriorPortals(true)
  _nPortal = nil
  _InitOutfitChange()
end

function NetSafeGetSpecialCaseGreeting()
  return "Fiona-Briefing-Contract-Oil020-13"
end

function _LoadStarters()
  _EnableStarters(true)
  _bPreOilCon020 = nil
  local sGreeting
  for sStarterId, tStarterData in pairs(_tStarters) do
    local oStarter = tStarterData.oStarter
    if oStarter then
      local tBriefings = oStarter:GetOfferedBriefings()
      if tBriefings.OilCon020 then
        _bPreOilCon020 = true
        _SetStarterChatter(sStarterId, false)
      end
      if tBriefings.OilCon020 and tBriefings.PmcCon031 then
        sGreeting = "Fiona-Briefing-Contract-Oil020-13"
      end
      oStarter:SetSpecialCaseGreeting(sGreeting)
    end
  end
  local nStartersLoaded = 0
  local nStartersToLoad = 0
  
  local function _StarterLoaded()
    nStartersLoaded = nStartersLoaded + 1
    if nStartersLoaded == nStartersToLoad then
      _Kickoff(1)
    end
  end
  
  for sStarterId, tStarterData in pairs(_tStarters) do
    if tStarterData.oStarter then
      nStartersToLoad = nStartersToLoad + 1
      tStarterData.oStarter:Load(_StarterLoaded)
    end
  end
  if nStartersToLoad == 0 then
    _Kickoff(1)
  end
end

function _BriefingModuleLoaded(mModule)
  _mBriefingModule = mModule
  _AssociateStartersToBriefingModule()
  _mBriefingModule.LoadTableOfAssets(_tAssetPreload, _Kickoff, {2})
end

function _AssociateStartersToBriefingModule()
  for sStarterId, tStarterData in pairs(_tStarters) do
    if tStarterData.oStarter then
      tStarterData.oStarter:SetBriefingModule(_mBriefingModule)
    end
  end
end

function _Kickoff(nSignal)
  if nSignal == 1 then
    _bHeroTeleportComplete = true
    MrxUtil.CallWithOptionalArgs(_fTeleportCallback, _tTeleportCallbackArgs)
    SetTeleportCallback(nil, nil)
  elseif nSignal == 2 then
    _bAssetsLoaded = true
  end
  if not _bHeroTeleportComplete or not _bAssetsLoaded then
    return
  end
  _bHeroTeleportComplete = nil
  _bAssetsLoaded = nil
  _EnablePortals(true, true)
  _UpdateStockpile(false)
  MrxUtil.CallWithOptionalArgs(_fLoadCallback, _tLoadCallbackArgs)
  SetLoadCallback(nil, nil)
  _bEntering = nil
  if _bWagerMissionComplete then
    local sStarterId = WifMissionData.GetMissionStarter(_sWagerMissionId)
    _StartStarter(sStarterId)
    _bWagerMissionComplete = nil
    _sWagerMissionId = nil
    _bWagerWin = nil
  else
    MrxState.Exit(MrxState.STATE_WAITFORGAME)
  end
  _SetupPreOilCon020Nag()
  _SetFakePDA(true)
end

function _SetFakePDA(bSet)
  if not bSet then
    Pda.Map:SetFakePlayerLocation({})
    local tPlayers = Player.GetAllPlayers()
    for _, uPlayer in ipairs(tPlayers) do
      Player.SetInPmc(uPlayer, false)
    end
    return
  end
  local tPlayers = Player.GetAllPlayers()
  local uRealPmc = Pg.GetGuidByName("_pmcoutpost_bld_hq_livedin 0x000d3c77")
  if uRealPmc == nil then
    uRealPmc = Pg.GetGuidByName("PMC_CentralBuilding")
  end
  local uFakePmc = Pg.GetGuidByName("HqInterior")
  local rX, rY, rZ = 0, 0, 0
  local fX, fY, fZ = 0, 0, 0
  if uRealPmc then
    rX, rY, rZ = Object.GetPosition(uRealPmc)
  end
  if uFakePmc then
    fX, fY, fZ = Object.GetPosition(uFakePmc)
  end
  for i, uPlayer in ipairs(tPlayers) do
    local uChar = Player.GetCharacter(uPlayer)
    if uChar then
      local cX, cY, cZ = Object.GetPosition(uChar)
      if cX and cY and cZ then
        cX = rX + (fX - cX)
        cY = rY + (fY - cY)
        cZ = rZ + (fZ - cZ)
        Pda.Map:SetFakePlayerLocation({
          vPlayer = uPlayer,
          nX = cX,
          nY = cY,
          nZ = cZ
        })
      end
    end
    Player.SetInPmc(uPlayer, true)
  end
end

function Exit(nPortal, bForReload)
  if not _bInside then
    return
  end
  _EnableInteriorPortals(false)
  _bInside = false
  if Net.IsServer() then
    Net.SetLoadingScreen(true)
    Event.Delete(_evClientJoinedPMC)
    Event.Delete(_evClientQuitPMC)
    if _evRemotePlayerAwake then
      Event.Delete(_evRemotePlayerAwake)
      _evRemotePlayerAwake = nil
    end
  end
  MrxState.Enter(MrxState.STATE_WAITFORGAME, _OnExit, {nPortal, bForReload})
end

function _OnExit(nPortal, bForReload)
  if _bExiting then
    return
  end
  _bExiting = true
  _nPortal = nPortal
  _sCurrentRoom = nil
  _EnablePortals(false, true)
  _EnableStarters(false)
  _SetPmcTransitLocation(true)
  _bWelcomedPlayer = nil
  MrxLayerManager.Remove(_tLayers, _ExitEnd, {1, bForReload}, true)
  if not bForReload then
    if _nPortal < 0 then
      _ExitEnd(2, false)
    else
      local sPendingContract
      for sStarterId, tStarterData in pairs(_tStarters) do
        local oStarter = tStarterData.oStarter
        if oStarter then
          sPendingContract = oStarter:GetPendingContract()
          if sPendingContract then
            break
          end
        end
      end
      local tExitTo
      if sPendingContract then
        tExitTo = WifMissionFlow.GetMissionStartLocations(sPendingContract)
        _DoParkingLot(sPendingContract)
      else
        local tPortalData = _tPortalData[_nPortal]
        tExitTo = {
          tPortalData.sExterior1,
          tPortalData.sExterior2
        }
      end
      MrxUtil.TeleportHeroesToLocations(tExitTo, _ExitEnd, {2, bForReload})
    end
  end
  _nPortal = nil
  _DeinitOutfitChange()
  _tViewedIntros = _mBriefingModule.GetViewedIntros()
  _mBriefingModule.UnloadTableOfAssets(_tAssetPreload, _ExitEnd, {3, bForReload})
  _mBriefingModule = nil
  if Net.IsServer() then
    Net.SetBriefingInterior()
  end
  dynamic_remove("MrxBriefing")
  WifPmcGarage.CheckFionaCar(true)
  _StopPreOilCon020Nag()
end

function _ExitEnd(nSignal, bForReload)
  if nSignal == 1 then
    Debug.Printf("@@@@@@@@@@ WifPmcInterior._ExitEnd: _bLayersRemoved")
    _bLayersRemoved = true
  elseif nSignal == 2 then
    Debug.Printf("@@@@@@@@@@ WifPmcInterior._ExitEnd: _bHeroTeleportComplete")
    _bHeroTeleportComplete = true
  elseif nSignal == 3 then
    Debug.Printf("@@@@@@@@@@ WifPmcInterior._ExitEnd: _bAssetsUnloaded")
    _bAssetsUnloaded = true
  end
  if not (_bLayersRemoved and (bForReload or _bHeroTeleportComplete)) or not _bAssetsUnloaded then
    return
  end
  _bLayersRemoved = nil
  _bHeroTeleportComplete = nil
  _bAssetsUnloaded = nil
  Event.Create(Event.TimerRelative, {2}, function()
    RefreshUiDisplay()
    WifFreePlay.StartNag()
    if not bForReload then
      local tAllMissionsToBeAccepted = {}
      local sLastAcceptedMission
      for sStarterId, tStarterData in pairs(_tStarters) do
        local oStarter = tStarterData.oStarter
        if oStarter then
          local tMissionsToBeAccepted, sLastAcceptedMission = oStarter:GetMissionsToBeAccepted()
          tAllMissionsToBeAccepted = MrxUtil.MergeIndexedTables(tAllMissionsToBeAccepted, tMissionsToBeAccepted)
          oStarter:ResetIntraSessionData()
          oStarter:Unload()
        end
      end
      if table.getn(tAllMissionsToBeAccepted) > 0 then
        WifMissionFlow.AcceptMissions(tAllMissionsToBeAccepted, sLastAcceptedMission)
      end
    end
    MrxState.Exit(MrxState.STATE_WAITFORGAME)
    MrxUtil.CallWithOptionalArgs(_fUnloadCallback, _tUnloadCallbackArgs)
    SetUnloadCallback(nil, nil)
  end)
  _SetFakePDA(false)
  MrxHq.GlobalExit()
  MrxState.Exit(MrxState.STATE_WAITFORGAME, _ExitComplete)
  _bExiting = nil
end

function _ExitComplete()
  if _tViewedIntros then
    for sIntro, _ in pairs(_tViewedIntros) do
      local tHqs = WifBriefingData.Intros[sIntro].tHq
      for _, sHq in ipairs(tHqs) do
        local sBlipName = sHq .. "::Portal"
        Hud.Radar:AnimateObjectiveSize({sName = sBlipName})
      end
    end
    _tViewedIntros = nil
  end
end

function NetSafeRemovePmcPdaBlip()
  RemovePmcPdaBlip()
end

function RemovePmcPdaBlip()
  Pda.Map:RemoveBlip({sName = "Pmc", bDontNetSync = true})
end

function NetSafeAddPmcPdaBlip(bSticky, tMissions)
  AddPmcPdaBlip(bSticky, tMissions)
end

function GetMissionDesc(tMissions)
  local sDesc = ""
  for _, sMissionName in pairs(tMissions) do
    local sTitle = WifMissionData.GetMissionTitle(sMissionName)
    local sFactionId = WifMissionData.GetMissionFaction(sMissionName)
    local sInlineIcon = ""
    if sFactionId then
      sInlineIcon = MrxFactionManager.GetInlineIcon(sFactionId) .. " "
    end
    sDesc = sDesc .. sInlineIcon .. "\"" .. sTitle .. "\""
    if WifMissionData.GetMissionRepeatable(sMissionName) then
      local nCompleted = WifMissionFlow.GetKeyValue(sMissionName)
      local nLevel = nCompleted + 1
      local nLevels = WifMissionData.GetMissionLevels(sMissionName)
      if nLevel > nLevels then
        nLevel = nLevels
      end
      sDesc = sDesc .. " ([Generic.Level] " .. nLevel .. ")"
    end
    sDesc = sDesc .. "\n"
  end
  return sDesc
end

function AddPmcPdaBlip(bSticky, tMissions)
  local uBlippedObject = Pg.GetGuidByName("Starter_Pmc_Start1")
  local nBriefings = 0
  local sDesc = ""
  if _tStarters.MecPmcBoss.oStarter then
    sDesc = sDesc .. "[cash] [Briefing.Shop]\n"
  end
  if _tStarters.HelPmcBoss.oStarter then
    sDesc = sDesc .. "[vehheli] [Briefing.Transit]\n"
  end
  if sDesc ~= "" then
    sDesc = sDesc .. "\n"
  end
  sDesc = sDesc .. "[PDA.Map.WorkAvailableHeader]\n"
  local tCriticalPathMission = {}
  local tNonCriticalPathMission = {}
  for _, nMissionIndex in pairs(tMissions) do
    local sMissionName = WifMissionData.GetMissionIdFromIndex(nMissionIndex)
    local bCriticalPath = WifMissionData.IsMissionOnCriticalPath(sMissionName)
    if bCriticalPath then
      table.insert(tCriticalPathMission, sMissionName)
    else
      table.insert(tNonCriticalPathMission, sMissionName)
    end
    nBriefings = nBriefings + 1
  end
  table.sort(tCriticalPathMission)
  table.sort(tNonCriticalPathMission)
  if nBriefings <= 0 then
    sDesc = sDesc .. "([Generic.None])"
  else
    if 0 < table.getn(tCriticalPathMission) then
      sDesc = sDesc .. GetMissionDesc(tCriticalPathMission)
    end
    if 0 < table.getn(tNonCriticalPathMission) then
      sDesc = sDesc .. GetMissionDesc(tNonCriticalPathMission)
    end
  end
  Pda.Map:RemoveBlip({sName = "Pmc"})
  local sPdaFaction = MrxFactionManager.GetPdaFactionIdFromFactionId("Pmc")
  Pda.Map:AddBlip({
    sName = "Pmc",
    uGuid = uBlippedObject,
    sLabel = "[PDA.Map.Locations.PMC]",
    sDesc = sDesc,
    sTexture = "icon_pmc_mc",
    bSticky = bSticky,
    sMission = nil,
    sFaction = sPdaFaction,
    bTodoList = false,
    bDontNetSync = true
  })
end

function RefreshUiDisplay()
  if Net.IsClient() then
    return
  end
  local bDisplay = true
  local bSticky = false
  local bIsFreePlay = MrxPlayState.IsFree()
  if _bEntranceLock then
    bDisplay = false
  end
  for sStarterId, tStarterData in pairs(_tStarters) do
    local oStarter = MrxStarterManager.GetStarter(sStarterId)
    if oStarter then
      local bHasUnviewedIntros = false
      local tIntros = oStarter:GetIntros()
      if tIntros then
        for sIntroName, _ in pairs(tIntros) do
          if not oStarter:HasViewedIntro(sIntroName) then
            bHasUnviewedIntros = true
            break
          end
        end
      end
      if oStarter:HasCriticalPathBriefings() or bHasUnviewedIntros then
        bSticky = true
      end
    end
  end
  bSticky = bSticky and bIsFreePlay
  if bDisplay then
    local tMissions = {}
    local nBriefings = 0
    for sStarterId, tStarterData in pairs(_tStarters) do
      local oStarter = MrxStarterManager.GetStarter(sStarterId)
      if oStarter then
        local tBriefings = oStarter:GetOfferedBriefings()
        for sMissionName, _ in pairs(tBriefings) do
          nBriefings = nBriefings + 1
          tMissions[nBriefings] = WifMissionData.GetMissionIndexFromId(sMissionName)
        end
      end
    end
    local iSticky = 0
    if bSticky ~= nil then
      if bSticky == true then
        iSticky = 1
      else
        iSticky = 2
      end
    end
    AddPmcPdaBlip(bSticky, tMissions)
    Net.SendEvent_AddPmcPdaBlip(iSticky, tMissions)
  end
  if not (bDisplay or _bDisplayed) or bDisplay and _bDisplayed and bSticky == _bSticky then
    return
  end
  _bDisplayed = bDisplay
  _bSticky = bSticky
  _EnablePortals(bDisplay, false)
  Hud.Radar:RemoveObjective({sName = "Pmc"})
  if bDisplay then
    local uBlippedObject = Pg.GetGuidByName("Starter_Pmc_Start1")
    Hud.Radar:AddObjective({
      sName = "Pmc",
      uGuid = uBlippedObject,
      nR = 255,
      nG = 255,
      nB = 255,
      nWidth = 8,
      nHeight = 8,
      sTexture = "MiniMap_Icon_Faction_PMC",
      bSticky = bSticky
    })
  else
    RemovePmcPdaBlip()
    Net.SendEvent_RemovePmcPdaBlip()
  end
end

function _SetPortalMarker(uGuid, bEnable)
  local tPortal = _tPortals[uGuid]
  if bEnable then
    local sTextureName = "HUD_HQ_PMC"
    if tPortal.bIsAnExit then
      sTextureName = "HUD_exit"
    end
    tPortal.uMarker = Marker.AddBlip(uGuid, sTextureName, 32, 255, 255, 255, 255, 1.25, 20, 30)
    if Net.IsServer() then
      Net.SendEvent_AddMarkerObjective(uGuid, tPortal.uMarker, 255, 255, 255, 1.25, MrxUtil.MarkerGetIndexByName_World(sTextureName), 1, 16, false, 20, 30)
    end
    local disc_r, disc_g, disc_b = MrxUtil.GetPrimaryObjectiveRgb()
    tPortal.uDisc = Marker.AddDisc(uGuid, 0.5, disc_r, disc_g, disc_b, 0.1)
    if Net.IsServer() then
      Net.SendEvent_AddMarkerObjective(uGuid, tPortal.uDisc, disc_r, disc_g, disc_b, 0.1, 0, 0.5, 0, true)
    end
  else
    if tPortal.uMarker then
      Marker.Remove(tPortal.uMarker)
      if Net.IsServer() then
        Net.SendEvent_RemoveMarkerObjective(tPortal.uMarker)
      end
      tPortal.uMarker = nil
    end
    if tPortal.uDisc then
      Marker.Remove(tPortal.uDisc)
    end
    if Net.IsServer() then
      Net.SendEvent_RemoveMarkerObjective(tPortal.uDisc)
    end
  end
end

function _AddPortal(uGuid, bExit, fCallback, tCallbackArgs)
  local tPortal = _tPortals[uGuid]
  if tPortal then
    return
  end
  _tPortals[uGuid] = {}
  tPortal = _tPortals[uGuid]
  tPortal.bIsAnExit = bExit
  
  local function _Go()
    if tPortal.uAwakeEvent then
      Event.Delete(tPortal.uAwakeEvent)
      tPortal.uAwakeEvent = nil
    end
    local sActionString
    if bExit then
      sActionString = "[ContextAction.Exit]"
    else
      sActionString = "[ContextAction.Enter]"
    end
    Pg.AddContextAction(uGuid, sActionString, 2, 0, 0, 255, 2, false)
    tPortal.uEvent = Event.CreatePersistent(Event.ContextAction, {0, uGuid}, fCallback, tCallbackArgs or {})
    _SetPortalMarker(uGuid, true)
  end
  
  if Object.IsAwake(uGuid) then
    _Go()
  else
    tPortal.uAwakeEvent = Event.Create(Event.ObjectHibernation, {uGuid, "awake"}, _Go)
  end
end

function _RemovePortal(uGuid)
  local tPortal = _tPortals[uGuid]
  if not tPortal then
    return
  end
  Pg.RemoveContextAction(uGuid)
  if tPortal.uEvent then
    Event.Delete(tPortal.uEvent)
    tPortal.uEvent = nil
  end
  if tPortal.uAwakeEvent then
    Event.Delete(tPortal.uAwakeEvent)
    tPortal.uAwakeEvent = nil
  end
  _SetPortalMarker(uGuid, false)
  _tPortals[uGuid] = nil
end

function _AddPortalAtHardpoint(sHardpoint, fCallback, tCallbackArgs)
  local sObjectName = sHardpoint .. "_portal"
  local uGuid = MrxUtil.SpawnActor("location", sObjectName, "HqInterior", sHardpoint, 0, false, false, function()
    _AddPortal(Pg.GetGuidByName(sObjectName), false, fCallback, tCallbackArgs)
  end)
  return uGuid
end

function _RemovePortalAtHardpoint(sHardpoint)
  local sObjectName = sHardpoint .. "_portal"
  local uGuid = Pg.GetGuidByName(sObjectName)
  _RemovePortal(uGuid)
  Object.Remove(uGuid)
end

function _EnablePortals(bEnable, bExits)
  for i, tPortalData in ipairs(_tPortalData) do
    local uGuid, fCallback, tCallbackArgs
    if bExits then
      uGuid = Pg.GetGuidByName(tPortalData.sInterior_Exit)
      fCallback = Exit
      tCallbackArgs = {i, false}
    else
      uGuid = Pg.GetGuidByName(tPortalData.sExterior_Entrance)
      fCallback = _OnEnter
      tCallbackArgs = {i}
    end
    if bEnable then
      _AddPortal(uGuid, bExits, fCallback, tCallbackArgs)
    else
      _RemovePortal(uGuid)
    end
  end
end

function _EnableInteriorPortals(bEnable)
  if not _sCurrentRoom then
    return
  end
  for i, tPortalData in ipairs(_tInteriorPortalData) do
    local uGuid
    if tPortalData.sOriginRoom == _sCurrentRoom then
      uGuid = Pg.GetGuidByName(tPortalData.sOrigin1)
    elseif tPortalData.sTerminusRoom == _sCurrentRoom then
      uGuid = Pg.GetGuidByName(tPortalData.sTerminus1)
    end
    if uGuid then
      if bEnable then
        _AddPortal(uGuid, false, _OnInteriorPortalEnter, {i})
      else
        _RemovePortal(uGuid)
      end
    end
  end
end

function _OnInteriorPortalEnter(nPortal)
  _EnableInteriorPortals(false)
  local tPortalData = _tInteriorPortalData[nPortal]
  local bOriginToTerminus, sDest1, sDest2
  if tPortalData.sOriginRoom == _sCurrentRoom then
    bOriginToTerminus = true
    sDest1 = tPortalData.sTerminus1
    sDest2 = tPortalData.sTerminus2
    _sCurrentRoom = tPortalData.sTerminusRoom
  else
    bOriginToTerminus = false
    sDest1 = tPortalData.sOrigin1
    sDest2 = tPortalData.sOrigin2
    _sCurrentRoom = tPortalData.sOriginRoom
  end
  _EnableInteriorPortals(true)
  MrxUtil.TeleportHeroesToLocations({sDest1, sDest2})
end

function _SetupStarters()
  for sStarterId, tStarterData in pairs(_tStarters) do
    tStarterData.oStarter = MrxStarterManager.GetStarter(sStarterId)
  end
end

function _EnableStarters(bEnable)
  for sStarterId, tStarterData in pairs(_tStarters) do
    if bEnable then
      _EnableStarter(sStarterId)
    else
      _DisableStarter(sStarterId)
    end
  end
end

function _SetStarterContextAction(uStarter, bEnable)
  if bEnable then
    Pg.AddContextAction(uStarter, "[ContextAction.Talk]", 4, 0, 0, 255, 4, false)
  else
    Pg.RemoveContextAction(uStarter)
  end
end

function _SetStarterMarker(sStarterId, bEnable)
  local tStarterData = _tStarters[sStarterId]
  if not tStarterData then
    return
  end
  local oStarter = tStarterData.oStarter
  local uStarter = tStarterData.uStarter
  if not oStarter or not uStarter then
    return
  end
  Debug.Printf("Setting (" .. tostring(bEnable) .. ") " .. sStarterId .. "'s marker")
  if bEnable then
    local bCriticalPath = false
    local tBriefings = oStarter:GetOfferedBriefings()
    if tBriefings then
      for sMissionName, _ in pairs(tBriefings) do
        if WifMissionData.IsMissionOnCriticalPath(sMissionName) then
          bCriticalPath = true
          break
        end
      end
    end
    local tIntros = oStarter:GetIntros()
    if tIntros then
      for sIntroName, _ in pairs(tIntros) do
        if not oStarter:HasViewedIntro(sIntroName) then
          bCriticalPath = true
          break
        end
      end
    end
    local IconR, IconG, IconB, nFadeDistanceNear, nFadeDistanceFar
    if bCriticalPath then
      IconR, IconG, IconB = MrxUtil.GetPrimaryObjectiveRgb()
      nFadeDistanceNear = nil
      nFadeDistanceFar = nil
    else
      IconR, IconG, IconB = MrxUtil.GetSecondaryObjectiveRgb()
      nFadeDistanceNear = 20
      nFadeDistanceFar = 30
    end
    tStarterData._uMarker = Marker.AddBlip(uStarter, tStarterData.sWldBlpTexture, 32, IconR, IconG, IconB, 255, 2, nFadeDistanceNear, nFadeDistanceFar)
    if Net.IsServer() then
      Net.SendEvent_AddMarkerObjective(uStarter, tStarterData._uMarker, IconR, IconG, IconB, 2, MrxUtil.MarkerGetIndexByName_World(tStarterData.sWldBlpTexture), 1, 16, false, nFadeDistanceNear, nFadeDistanceFar)
    end
  elseif tStarterData._uMarker then
    Marker.Remove(tStarterData._uMarker)
    if Net.IsServer() then
      Net.SendEvent_RemoveMarkerObjective(tStarterData._uMarker)
    end
    tStarterData._uMarker = nil
  end
end

function _SetStarterChatter(sStarterId, bEnable)
  local tStarterData = _tStarters[sStarterId]
  if not tStarterData then
    return
  end
  local oStarter = tStarterData.oStarter
  local uStarter = tStarterData.uStarter
  if not oStarter or not uStarter then
    return
  end
  Debug.Printf("Setting (" .. tostring(bEnable) .. ") " .. sStarterId .. "'s chatter")
  if bEnable then
    tStarterData._uChatterEvent = Event.Create(Event.ObjectProximity, {
      Player.GetAnyCharacter(),
      uStarter,
      "<",
      7
    }, _StarterChatter, {sStarterId})
    tStarterData._bChatterEnabled = true
  else
    Event.Delete(tStarterData._uChatterEvent)
    tStarterData._uChatterEvent = nil
    tStarterData._bChatterEnabled = nil
    if _sCurrentStarterChatter == sStarterId then
      _sCurrentStarterChatter = nil
      Debug.Printf("Canceling " .. sStarterId .. "'s chatter")
      MrxVoSequence.Stop(false, true, MrxVoSequence.knPriorityFreeplay)
    end
  end
end

function _StarterChatter(sStarterId)
  local tStarterData = _tStarters[sStarterId]
  local uStarter = tStarterData.uStarter
  
  function _ChatterComplete(sStarterId)
    local tStarterData = _tStarters[sStarterId]
    if not tStarterData._bChatterEnabled then
      return
    end
    tStarterData._uChatterEvent = Event.Create(Event.TimerRelative, {20}, _SetStarterChatter, {sStarterId, true})
    if _sCurrentStarterChatter == sStarterId then
      _sCurrentStarterChatter = nil
    end
  end
  
  if _sCurrentStarterChatter then
    _ChatterComplete(sStarterId)
  else
    local sVo = _GetStarterChatterVo(sStarterId)
    Debug.Printf("Playing " .. sStarterId .. "'s (" .. tostring(uStarter) .. ") chatter - " .. sVo)
    local tSequence = {
      {sVo, uStarter},
      {
        _ChatterComplete,
        {sStarterId}
      }
    }
    MrxVoSequence.Start(tSequence, false, MrxVoSequence.knPriorityFreeplay)
    Human.DoAction(uStarter, "Proximity")
    _sCurrentStarterChatter = sStarterId
  end
end

function _EnableStarter(sStarterId)
  local tStarterData = _tStarters[sStarterId]
  if tStarterData.bEnabled then
    return
  end
  local oStarter = tStarterData.oStarter
  local sSourceObject = tStarterData.sSourceObject
  if not oStarter or not sSourceObject then
    return
  end
  local uSourceObject = Pg.GetGuidByName(sSourceObject)
  local tRiders = Vehicle.GetRiders(uSourceObject)
  local uStarter = tRiders[1]
  tStarterData.uStarter = uStarter
  oStarter:SetActor(uStarter)
  _SetStarterContextAction(uStarter, true)
  tStarterData.uEvent = Event.CreatePersistent(Event.ContextAction, {0, uStarter}, MrxState.Enter, {
    MrxState.STATE_WAITFORGAME,
    _StartStarter,
    {sStarterId}
  })
  _SetStarterMarker(sStarterId, true)
  _SetStarterChatter(sStarterId, true)
  tStarterData.bEnabled = true
end

function _DisableStarter(sStarterId)
  local tStarterData = _tStarters[sStarterId]
  if not tStarterData or not tStarterData.bEnabled then
    return
  end
  local oStarter = tStarterData.oStarter
  oStarter:SetActor(nil)
  _SetStarterContextAction(tStarterData.uStarter, false)
  if tStarterData.uEvent then
    Event.Delete(tStarterData.uEvent)
    tStarterData.uEvent = nil
  end
  _SetStarterMarker(sStarterId, false)
  _SetStarterChatter(sStarterId, false)
  tStarterData.bEnabled = nil
end

function _StartStarter(sStarterId)
  local tStarterData = _tStarters[sStarterId]
  local oStarter = tStarterData.oStarter
  local sSourceObject = tStarterData.sSourceObject
  local uSourceObject = Pg.GetGuidByName(sSourceObject)
  local tRiders = Vehicle.GetRiders(uSourceObject)
  local uStarter = tRiders[1]
  tStarterData.uStarter = uStarter
  _sCurrentStarterId = sStarterId
  Vehicle.Exit(uSourceObject, uStarter)
  _SetStarterContextAction(uStarter, false)
  _SetAllMarkers(false)
  _SetAllStarterChatter(false)
  oStarter:Start()
  _StopPreOilCon020Nag()
end

function BriefingComplete(bDontResetStarters)
  if not _sCurrentStarterId then
    return
  end
  local tStarterData = _tStarters[_sCurrentStarterId]
  _sCurrentStarterId = nil
  if not bDontResetStarters and tStarterData.uStarter then
    local sSourceObject = tStarterData.sSourceObject
    local uSourceObject = Pg.GetGuidByName(sSourceObject)
    Vehicle.Enter(uSourceObject, tStarterData.uStarter)
    _SetStarterContextAction(tStarterData.uStarter, true)
    _SetAllMarkers(true)
    if _bPreOilCon020 then
      _SetupPreOilCon020Nag()
    else
      Event.Create(Event.TimerRelative, {2}, _SetAllStarterChatter, {true})
    end
  end
end

function IsInside()
  return _bInside
end

function IsEntering()
  return _bEntering
end

function IsContractPending()
  local bContractPending = false
  for sStarterId, tStarterData in pairs(_tStarters) do
    if tStarterData.oStarter and not bContractPending then
      bContractPending = tStarterData.oStarter:IsContractPending()
    end
  end
end

function GetStarterBriefingLocs(sStarterId)
  local tStarterData = _tStarters[sStarterId]
  if not tStarterData then
    return
  end
  return {
    tStarterData.sBriefingLoc,
    tStarterData.sHero1BriefingLoc
  }
end

function SetLoadCallback(fCallback, tCallbackArgs)
  _fLoadCallback = fCallback
  _tLoadCallbackArgs = tCallbackArgs
end

function SetUnloadCallback(fCallback, tCallbackArgs)
  _fUnloadCallback = fCallback
  _tUnloadCallbackArgs = tCallbackArgs
end

function SetTeleportCallback(fCallback, tCallbackArgs)
  _fTeleportCallback = fCallback
  _tTeleportCallbackArgs = tCallbackArgs
end

function _SetCustomOutfitMarker(bEnable)
  local uGuid = Pg.GetGuidByName("Custom Outfit Location")
  if bEnable then
    Debug.Printf("Enabling Custom Outfit...")
    Pg.AddContextAction(uGuid, "[ContextAction.ChangeOutfit]", 1)
    _uOutfitEvent = Event.CreatePersistent(Event.ContextAction, {0, uGuid}, _SelectOutfit)
    local sTextureName = "HUD_wardrobe"
    _uOutfitMarker = Marker.AddBlip(uGuid, sTextureName, 32, 255, 255, 255, 255, 1.25, 20, 30)
    if Net.IsServer() then
      Net.SendEvent_AddMarkerObjective(uGuid, _uOutfitMarker, 255, 255, 255, 1.25, MrxUtil.MarkerGetIndexByName_World(sTextureName), 1, 16, false, 20, 30)
    end
    local disc_r, disc_g, disc_b = MrxUtil.GetPrimaryObjectiveRgb()
    _uOutfitDisc = Marker.AddDisc(uGuid, 0.5, disc_r, disc_g, disc_b, 0.1)
    if Net.IsServer() then
      Net.SendEvent_AddMarkerObjective(uGuid, _uOutfitDisc, disc_r, disc_g, disc_b, 0.1, 0, 0.5, 0, true)
    end
  else
    Debug.Printf("Disabling Custom Outfit...")
    Pg.RemoveContextAction(uGuid)
    Event.Delete(_uOutfitEvent)
    _uOutfitEvent = nil
    if _uOutfitMarker then
      Marker.Remove(_uOutfitMarker)
      if Net.IsServer() then
        Net.SendEvent_RemoveMarkerObjective(_uOutfitMarker)
      end
      _uOutfitMarker = nil
    end
    if _uOutfitDisc then
      Marker.Remove(_uOutfitDisc)
    end
    if Net.IsServer() then
      Net.SendEvent_RemoveMarkerObjective(_uOutfitDisc)
    end
  end
end

function _InitOutfitChange()
  local uWardrobe = Pg.GetGuidByName("Custom Outfit Location")
  if not uWardrobe then
    Debug.Printf("Cannot find Custom Outfit Wardrobe in PMC, disabling Custom Outfits")
    return
  end
  if Net.IsServer() then
    _evClientQuitPMC = Event.CreatePersistent(Event.ScriptEvent, {
      "mpPlayerLeft",
      function(tData)
        return Net.IsServer() and not Player.IsLocal(tData[1])
      end
    }, _ReinitOutfitChange)
  end
  _SetCustomOutfitMarker(true)
end

function _DeinitOutfitChange()
  local uWardrobe = Pg.GetGuidByName("Custom Outfit Location")
  if not uWardrobe then
    return
  end
  _SetCustomOutfitMarker(false)
end

function _ReinitOutfitChange()
  if not _WardrobeOpen then
    _DeinitOutfitChange()
    _InitOutfitChange()
  end
end

_CostumeDialogBox = nil
_WardrobeOpen = false

function _SelectOutfit(uGuid)
  if uGuid == Player.GetPrimaryCharacter() then
    _WardrobeOpen = true
  end
  if Net.IsServer() then
    _SetCustomOutfitMarker(false)
  end
  if Net.IsServer() and uGuid == Player.GetSecondaryCharacter() then
    Net.SendCustomEvent("WifPmcInterior", NETEVENT_CHANGEOUTFIT, {})
    return
  end
  local sHero = MrxUtil.GetCharacterIdentity(uGuid)
  local tOutfits = _tOutfits[sHero]
  local nCurrentOutfit = Player.GetProfileCostume()
  if nCurrentOutfit == nil or nCurrentOutfit == 0 then
    nCurrentOutfit = 0
  end
  nCurrentOutfit = nCurrentOutfit + 1
  local nAvailableOutfits = 0
  if Net.IsClient() then
    nAvailableOutfits = Player.GetAvailableCostumes()
  else
    nAvailableOutfits = GetAvailableCostumes()
  end
  if nAvailableOutfits == nil or nAvailableOutfits == 0 then
    nAvailableOutfits = 0
  end
  if nAvailableOutfits <= 1 and not Net.HasPlayerUnlockedCode() then
    LTILibName.ChangeShellState(true)
    _CostumeDialogBox = MrxGui.DisplayDialogBox(Player.GetLocalPlayer(), "[Tutorial.CustomOutfit]", {
      "[Generic.Continue]"
    }, 1, _CloseCostumeDialog, {}, 48, 36, "left", "bottom", false, 1)
    return
  end
  nAvailableOutfits = nAvailableOutfits + 1
  MrxMultiPageMenu.Reset()
  if Net.HasPlayerUnlockedCode() and nCurrentOutfit ~= 2 then
    MrxMultiPageMenu.AddOption(tOutfits[2].PlayerVisibleName, _ChangeOutfit, {uGuid, 2})
  end
  for i, tOutfit in ipairs(tOutfits) do
    if i ~= nCurrentOutfit and i ~= 2 and nAvailableOutfits >= i then
      MrxMultiPageMenu.AddOption(tOutfit.PlayerVisibleName, _ChangeOutfit, {uGuid, i})
    end
  end
  if Net.IsServer() then
    MrxMultiPageMenu.AddOption("[Generic.Cancel]", _SetCustomOutfitMarker, {true}, true, true)
  elseif Net.IsClient() then
    MrxMultiPageMenu.AddOption("[Generic.Cancel]", Net.SendCustomEvent, {
      "WifPmcInterior",
      NETEVENT_NOTIFYOUTFITCHANGE,
      {0, 0}
    }, true, true)
  end
  MrxMultiPageMenu.Display("[Generic.ChooseOutfit]:")
end

function _CloseCostumeDialog()
  _CostumeDialogBox = nil
  LTILibName.ChangeShellState(false)
  if Net.IsServer() then
    _SetCustomOutfitMarker(true)
  elseif Net.IsClient() then
    Net.SendCustomEvent("WifPmcInterior", NETEVENT_NOTIFYOUTFITCHANGE, {0, 0})
  end
end

function _ChangeOutfit(uGuid, iIndex, fCallback, tCallbackArgs)
  if uGuid == Player.GetPrimaryCharacter() then
    _WardrobeOpen = false
  end
  MrxState.Enter(MrxState.STATE_WAITFORGAME, _CompleteChangeOutfit, {
    uGuid,
    iIndex,
    fCallback,
    tCallbackArgs
  })
end

function _CompleteChangeOutfit(uGuid, iIndex, fCallback, tCallbackArgs)
  local sHero = MrxUtil.GetCharacterIdentity(uGuid)
  local tOutfits = _tOutfits[sHero]
  local sModelName = tOutfits[iIndex].Model
  Player.SetProfileCostume(iIndex - 1)
  Player.SetOutfit(uGuid, sModelName)
  if uGuid == Player.GetPrimaryCharacter() then
    iPlayer = 1
  elseif uGuid == Player.GetSecondaryCharacter() then
    iPlayer = 2
  end
  Net.SendCustomEvent("WifPmcInterior", NETEVENT_NOTIFYOUTFITCHANGE, {iPlayer, iIndex})
  Event.Create(Event.ObjectIsReady, {
    uGuid,
    "awake",
    0
  }, function()
    local sVo = _GetPreeningVo(sHero, tOutfits[iIndex].Name)
    local fCue, tData
    if sVo then
      fCue = MrxVoSequence.Start
      tData = {
        {
          {sVo, uGuid}
        },
        false,
        MrxVoSequence.knPriorityFreeplay
      }
      Debug.Printf("Playing " .. sHero .. "'s Preening VO - " .. sVo)
    end
    _bChangedOutfit = true
    MrxState.Exit(MrxState.STATE_WAITFORGAME, fCue, tData)
    MrxUtil.CallWithOptionalArgs(fCallback, tCallbackArgs)
    if Net.IsServer() then
      _SetCustomOutfitMarker(true)
    end
  end)
end

function ChangeOutfit(uGuid, sOutfitName, fCallback, tCallbackArgs)
  local sHero = MrxUtil.GetCharacterIdentity(uGuid)
  local tOutfits = _tOutfits[sHero]
  for i, tOutfit in ipairs(tOutfits) do
    if tOutfit.Name == sOutfitName then
      _ChangeOutfit(uGuid, i, fCallback, tCallbackArgs)
      break
    end
  end
end

function GetAvailableCostumes()
  return _nAvailableCostumes or 1
end

function SetAvailableCostumes(nAvailableCostumes)
  local nUnlockedCostumes = nAvailableCostumes - GetAvailableCostumes()
  _nAvailableCostumes = nAvailableCostumes
  local tUnlockedCostumes = {}
  if 1 <= nUnlockedCostumes then
    local sHero = MrxUtil.GetCharacterIdentity(Player.GetLocalCharacter())
    local tOutfits = _tOutfits[sHero]
    local nEnd = _nAvailableCostumes + 1
    local nStart = nEnd - nUnlockedCostumes + 1
    for i = nStart, nEnd do
      local tOutfit = tOutfits[i]
      if tOutfit and tOutfit.Name then
        table.insert(tUnlockedCostumes, tOutfit.PlayerVisibleName)
      end
    end
  end
  return tUnlockedCostumes
end

function _UpdateStockpile(bClientJoined)
  local stockPileEventGroup = 1
  local stockPileEventItem = 1
  local tQty = {}
  for sName, tLimitData in pairs(_tStockpile) do
    local nQty
    if sName == "money" then
      nQty = MrxPmc.GetCashQty()
    else
      nQty = MrxPmc.GetSupportQty(sName)
      if nQty == nil then
        nQty = 0
      end
    end
    local nOldQty = _tStockpileQty[sName]
    if nOldQty == nil then
      nOldQty = -1
    end
    if nQty > nOldQty then
      _tStockpileQty[sName] = nQty
    else
      nQty = nOldQty
    end
    tQty[stockPileEventItem] = nQty
    if bClientJoined == false then
      _SetStockpileCategoryQty(sName, nQty)
    end
    if stockPileEventItem == 4 then
      if Net.IsServer() then
        Net.SendCustomEvent("WifPmcInterior", NETEVENT_UPDATESTOCKPILE, {
          stockPileEventGroup,
          tQty[1],
          tQty[2],
          tQty[3],
          tQty[4]
        }, bClientJoined)
        tQty[1] = nil
        tQty[2] = nil
        tQty[3] = nil
        tQty[4] = nil
      end
      stockPileEventItem = 1
      stockPileEventGroup = stockPileEventGroup + 1
    else
      stockPileEventItem = stockPileEventItem + 1
    end
  end
  if Net.IsServer() and 1 < stockPileEventItem then
    Net.SendCustomEvent("WifPmcInterior", NETEVENT_UPDATESTOCKPILE, {
      stockPileEventGroup,
      tQty[1],
      tQty[2],
      tQty[3],
      tQty[4]
    }, bClientJoined)
  end
end

function _SetStockpileCategoryQty(sCategoryName, nQty)
  Debug.Printf("_SetStockpileCategoryQty(" .. tostring(sCategoryName) .. ", " .. tostring(nQty) .. ")")
  local tLimitData = _tStockpile[sCategoryName]
  local sObjectName = "pmcoutpost_stockpile_" .. sCategoryName .. " "
  for i, nLimit in ipairs(tLimitData) do
    if nQty < nLimit then
      local uGuid = Pg.GetGuidByName(sObjectName .. i)
      if uGuid then
        Event.Create(Event.ObjectHibernation, {uGuid, "awake"}, function()
          Object.DisablePhysics(uGuid)
          Object.SetVisible(uGuid, false)
        end)
      end
    end
  end
end

function SaveSingleton()
  local tIntroduced
  for sStarterId, tStarterData in pairs(_tStarters) do
    if sStarterId ~= "PmcBoss" and tStarterData.bIntroduced then
      tIntroduced = tIntroduced or {}
      tIntroduced[sStarterId] = true
    end
  end
  return {
    bUnlocked = _bUnlocked,
    tStockpileQty = _tStockpileQty,
    tIntroduced = tIntroduced
  }
end

function LoadSingleton(tSaveData)
  if tSaveData.bUnlocked then
    Unlock()
    WifPmcGarage.CheckFionaCar(true)
  end
  if tSaveData.tSupportQty then
    _tStockpileQty = tSaveData.tStockpileQty
  end
  if tSaveData.tIntroduced then
    for sStarterId, bIntroduced in pairs(tSaveData.tIntroduced) do
      _tStarters[sStarterId].bIntroduced = bIntroduced
    end
  end
end

function _OnPlayerJoined()
  _UpdateStockpile(true)
  _ReinitOutfitChange()
  MrxUtil.EnableHeroWeapons(false)
end

NETEVENT_UPDATESTOCKPILE = 0
NETEVENT_CHANGEOUTFIT = 1
NETEVENT_NOTIFYOUTFITCHANGE = 2
_NetSafeBriefingModule = nil

function NetSafeBriefingModuleLoaded(mModule)
  _NetSafeBriefingModule = mModule
  _NetSafeBriefingModule.NetSafeLoadBriefingAssets(_tAssetPreload)
  MrxState.Exit(MrxState.STATE_WAITFORGAME)
end

function NetSafeLoadAssets1()
  dynamic_import("MrxBriefing", NetSafeBriefingModuleLoaded)
end

function NetSafeLoadAssets()
  _ClientOnEnter()
  MrxState.Enter(MrxState.STATE_WAITFORGAME, NetSafeLoadAssets1)
end

function NetSafeUnloadAssets()
  _ClientExit()
  if _NetSafeBriefingModule then
    _NetSafeBriefingModule.NetSafeUnloadBriefingAssets(_tAssetPreload)
    dynamic_remove("MrxBriefing")
    _NetSafeBriefingModule = nil
  end
end

function NetEventCallback(nEventType, tArgs)
  if nEventType == NETEVENT_UPDATESTOCKPILE then
    Debug.Printf("Received NETEVENT_UPDATESTOCKPILE")
    Debug.Printf("- tArgs[1] = " .. tostring(tArgs[1]))
    Debug.Printf("- tArgs[2] = " .. tostring(tArgs[2]))
    Debug.Printf("- tArgs[3] = " .. tostring(tArgs[3]))
    Debug.Printf("- tArgs[4] = " .. tostring(tArgs[4]))
    Debug.Printf("- tArgs[5] = " .. tostring(tArgs[5]))
    local stockPileEventGroup = 1
    local stockPileEventItem
    local nCount = 1
    for sName, tLimitData in pairs(_tStockpile) do
      if stockPileEventGroup == tArgs[1] and not stockPileEventItem then
        stockPileEventItem = 1
      end
      if stockPileEventItem and tArgs[1 + stockPileEventItem] then
        _SetStockpileCategoryQty(sName, tArgs[1 + stockPileEventItem])
        stockPileEventItem = stockPileEventItem + 1
      end
      nCount = nCount + 1
      if nCount == 5 then
        stockPileEventGroup = stockPileEventGroup + 1
        nCount = 1
        stockPileEventItem = nil
      end
    end
  elseif nEventType == NETEVENT_CHANGEOUTFIT then
    _SelectOutfit(Player.GetLocalCharacter())
  elseif nEventType == NETEVENT_NOTIFYOUTFITCHANGE then
    local uGuid = 0
    if tArgs[1] == 0 then
      _SetCustomOutfitMarker(true)
      return
    elseif tArgs[1] == 1 then
      uGuid = Player.GetPrimaryCharacter()
    elseif tArgs[1] == 2 then
      uGuid = Player.GetSecondaryCharacter()
    end
    local sHero = MrxUtil.GetCharacterIdentity(uGuid)
    local tOutfits = _tOutfits[sHero]
    local sModelName = tOutfits[tArgs[2]].Model
    Player.SetOutfit(uGuid, sModelName)
    if Net.IsServer() then
      _SetCustomOutfitMarker(true)
    end
  end
end

function _ClientOnEnter()
  MrxHq.GlobalEnter()
  Graphics.Atmosphere.ChangeLineRegionSetting(Pg.GetGuidByName("rgn_atmo_interior"), "pmc")
  _SetFakePDA(true)
  Graphics.Camera.SetNearFar(0, 0.3, 500, 0)
  MrxGuiInterface.HudInterface.FanfareQueue.ClientPause(false)
end

function _ClientExit()
  MrxMultiPageMenu.Close()
  if _CostumeDialogBox then
    LTILibName.ChangeShellState(false)
    MrxGui.CloseDialogBox(_CostumeDialogBox)
    _CostumeDialogBox = nil
  end
  MrxHq.GlobalExit()
  _SetFakePDA(false)
  Graphics.Camera.RestoreNearFar(0)
end

function _SendStockpileToClient(uGuid)
  Net.SendCustomEvent("WifPmcInterior", NETEVENT_UPDATESTOCKPILE, {uGuid})
end

function _ClientUpdateStockpile(uGuid)
  Object.DisablePhysics(uGuid)
  Object.SetVisible(uGuid, false)
end

function _SetupPreOilCon020Nag(nDelay)
  if not _bPreOilCon020 then
    return
  end
  if nDelay == nil then
    nDelay = 5
  end
  Debug.Printf("# Setting up PreOilCon020Nag with ", nDelay, "s delay")
  _uNagTimer = Event.Create(Event.TimerRelative, {nDelay}, _PreOilCon020Nag)
end

function _StopPreOilCon020Nag()
  if _uNagTimer then
    Event.Delete(_uNagTimer)
    _uNagTimer = nil
  end
  if _bNagInProgress then
    VO.Cancel(_tStarters.PmcBoss.uStarter)
  end
end

function _PreOilCon020Nag()
  local function _NagComplete(sResult)
    _bNagInProgress = false
    
    if sResult == "complete" then
      _SetupPreOilCon020Nag(30)
    end
  end
  
  _StopPreOilCon020Nag()
  _bNagInProgress = true
  local tVo = {
    "Fiona.AtPMC.01",
    "Fiona.AtPMC.02",
    "Fiona.AtPMC.03",
    "Fiona.AtPMC.04",
    "Fiona.AtPMC.05"
  }
  VO.Cue(_tStarters.PmcBoss.uStarter, MrxUtil.GetRandomTableElement(tVo), _NagComplete)
end

function _SetAllMarkers(bEnable)
  for sStarterId, tStarterData in pairs(_tStarters) do
    _SetStarterMarker(sStarterId, bEnable)
  end
  for uPortal, _ in pairs(_tPortals) do
    _SetPortalMarker(uPortal, bEnable)
  end
end

function _GetStarterChatterVo(sStarterId)
  local tAllVo = {
    PmcBoss = {
      CustomOutfit = {
        "Fiona.Misc.CustomOutfit01"
      },
      Greeting = {
        "Fiona.PMC.Greeting05",
        "Fiona.PMC.Greeting06",
        "Fiona.PMC.Greeting07",
        "Fiona.PMC.Greeting08",
        "Fiona.PMC.Greeting09"
      },
      Intros = {
        HelPmcBoss = {
          "Fiona.Cam.57"
        },
        MecPmcBoss = {
          "Fiona.Cam.58"
        },
        JetPmcBoss = {
          "Fiona.Cam.59"
        }
      }
    },
    HelPmcBoss = {
      CustomOutfit = {
        "Ewan.Misc.CustomOutfit01"
      },
      Greeting = {
        "Ewan.PMC.Greeting01",
        "Ewan.PMC.Greeting02",
        "Ewan.PMC.Greeting03",
        "Ewan.PMC.Greeting04",
        "Ewan.PMC.Greeting05",
        "Ewan.PMC.Greeting06",
        "Ewan.PMC.Greeting07",
        "Ewan.PMC.Greeting08",
        "Ewan.PMC.Greeting09",
        "Ewan.PMC.Greeting10"
      }
    },
    MecPmcBoss = {
      CustomOutfit = {
        "Eva.Misc.CustomOutfit01"
      },
      Greeting = {
        "Eva.PMC.Greeting01",
        "Eva.PMC.Greeting02",
        "Eva.PMC.Greeting03",
        "Eva.PMC.Greeting04",
        "Eva.PMC.Greeting05",
        "Eva.PMC.Greeting07",
        "Eva.PMC.Greeting08",
        "Eva.PMC.Greeting09",
        "Eva.PMC.Greeting10"
      }
    },
    JetPmcBoss = {
      CustomOutfit = {
        "Misha.Misc.CustomOutfit01"
      },
      Greeting = {
        "Misha.PMC.Greeting01",
        "Misha.PMC.Greeting02",
        "Misha.PMC.Greeting03",
        "Misha.PMC.Greeting04",
        "Misha.PMC.Greeting05",
        "Misha.PMC.Greeting06",
        "Misha.PMC.Greeting07",
        "Misha.PMC.Greeting08",
        "Misha.PMC.Greeting09",
        "Misha.PMC.Greeting10"
      }
    }
  }
  local bSpecialCase = true
  local tStarterVo = tAllVo[sStarterId]
  local tVo
  if _bChangedOutfit then
    tVo = tStarterVo.CustomOutfit
    _bChangedOutfit = nil
  end
  if sStarterId == "PmcBoss" then
    for sStarterId, tStarterData in pairs(_tStarters) do
      if sStarterId ~= "PmcBoss" and tStarterData.oStarter and not tStarterData.bIntroduced then
        tVo = tStarterVo.Intros[sStarterId]
        tStarterData.bIntroduced = true
        Debug.Printf("Introducing starter " .. sStarterId .. "...")
        break
      end
    end
  end
  if not tVo then
    bSpecialCase = false
    tVo = tStarterVo.Greeting
    if sStarterId == "PmcBoss" then
      if _tStarters.MecPmcBoss.oStarter and _tStarters.HelPmcBoss.oStarter then
        table.insert(tVo, "Fiona.PMC.Greeting02")
      end
      if MrxPmc.GetCashQty() > 1000000 then
        table.insert(tVo, "Fiona.PMC.Greeting10")
      end
      if WifMissionFlow.HasKey("PmcCon002") then
        table.insert(tVo, "Fiona.PMC.Greeting01")
      end
      if not _bWelcomedPlayer then
        if not WifMissionFlow.HasKey("PmcCon002") then
          table.insert(tVo, "Fiona.PMC.Greeting04")
        end
        if MrxPmc.GetCashQty() > 1000000 then
          table.insert(tVo, "Fiona.PMC.Greeting03")
        end
        _bWelcomedPlayer = true
      end
    elseif sStarterId == "MecPmcBoss" and _tStarters.JetPmcBoss.oStarter then
      table.insert(tVo, "Eva.PMC.Greeting06")
    end
  end
  if not tVo then
    return nil
  end
  if bSpecialCase then
    return MrxUtil.GetRandomTableElement(tVo)
  end
  local nIndex = _tStarters[sStarterId].nLastChatter
  if nIndex == nil or nIndex > table.getn(tVo) then
    nIndex = 1
  else
    nIndex = nIndex + 1
    if not tVo[nIndex] then
      nIndex = 1
    end
  end
  _tStarters[sStarterId].nLastChatter = nIndex
  return tVo[nIndex]
end

function _SetAllStarterChatter(bEnable)
  for sStarterId, tStarterData in pairs(_tStarters) do
    _SetStarterChatter(sStarterId, bEnable)
  end
end

function _GetPreeningVo(sCharacter, sOutfit)
  local tAllVo = {
    mattias = {
      Original = {
        "Mattias.CustomOutfit.01",
        "Mattias.CustomOutfit.02",
        "Mattias.CustomOutfit.03",
        "Mattias.CustomOutfit.04"
      },
      ChickenSuit = {
        "Mattias.CustomOutfit.Chicken.01",
        "Mattias.CustomOutfit.Chicken.02",
        "Mattias.CustomOutfit.Chicken.03",
        "Mattias.CustomOutfit.Chicken.04",
        "Mattias.CustomOutfit.Chicken.05"
      },
      Suit = {
        "Mattias.CustomOutfit.01",
        "Mattias.CustomOutfit.02",
        "Mattias.CustomOutfit.03",
        "Mattias.CustomOutfit.04"
      },
      MetalHead = {
        "Mattias.CustomOutfit.01",
        "Mattias.CustomOutfit.02",
        "Mattias.CustomOutfit.03",
        "Mattias.CustomOutfit.04"
      },
      Jacket = {
        "Mattias.CustomOutfit.01",
        "Mattias.CustomOutfit.02",
        "Mattias.CustomOutfit.03",
        "Mattias.CustomOutfit.04"
      }
    },
    chris = {
      Original = {
        "Chris.CustomOutfit.Generic01"
      },
      ChickenSuit = {
        "Chris.CustomOutfit.Chicken.01",
        "Chris.CustomOutfit.Chicken.02",
        "Chris.CustomOutfit.Chicken.03",
        "Chris.CustomOutfit.Chicken.04",
        "Chris.CustomOutfit.Chicken.05"
      },
      Vacation = {
        "Chris.CustomOutfit.Magnum01",
        "Chris.CustomOutfit.Magnum02",
        "Chris.CustomOutfit.Generic01"
      },
      Commando = {
        "Chris.CustomOutfit.Rambo01",
        "Chris.CustomOutfit.Rambo02",
        "Chris.CustomOutfit.Generic01"
      },
      OffDuty = {
        "Chris.CustomOutfit.Magnum02",
        "Chris.CustomOutfit.Generic01"
      }
    },
    jennifer = {
      Original = {
        "Jen.CustomOutfit.Styleish01"
      },
      ChickenSuit = {
        "Jen.CustomOutfit.Chicken.01",
        "Jen.CustomOutfit.Chicken.02",
        "Jen.CustomOutfit.Chicken.03",
        "Jen.CustomOutfit.Chicken.04",
        "Jen.CustomOutfit.Chicken.05"
      },
      Rebel = {
        "Jen.CustomOutfit.Army01"
      },
      CatSuit = {
        "Jen.CustomOutfit.Catsuit01",
        "Jen.CustomOutfit.Styleish01"
      },
      NoJacket = {
        "Jen.CustomOutfit.Jean01",
        "Jen.CustomOutfit.Styleish01"
      },
      Tactical = {
        "Jen.CustomOutfit.Army01",
        "Jen.CustomOutfit.Styleish01"
      }
    }
  }
  local tCharacterVo = tAllVo[sCharacter]
  if sOutfit == "Chicken Suit" then
    sOutfit = "ChickenSuit"
  elseif sOutfit == "NoJacket" then
    sOutfit = "NoJacket"
  elseif sOutfit == "Cat Suit" then
    sOutfit = "CatSuit"
  end
  local tVo = tCharacterVo[sOutfit]
  if not tVo then
    return
  end
  return MrxUtil.GetRandomTableElement(tVo)
end

function _OnPmcDeath(nBuilding)
  if not _tBuildingStates[nBuilding] then
    return
  end
  local uBuilding = Pg.GetGuidByName(_tBuildings[nBuilding])
  Debug.Printf("Pmc Building " .. nBuilding .. " (" .. tostring(uBuilding) .. ") has died!")
  _tBuildingStates[nBuilding] = false
  if nBuilding == 1 then
    _EnablePortals(false, false)
  elseif nBuilding == 2 then
    _SetPmcTransitLocation(false)
  end
  _tBuildingEvents[nBuilding] = Event.Create(Event.ObjectHibernation, {uBuilding, "s"}, _OnPmcHibernation, {nBuilding})
end

function _OnPmcHibernation(nBuilding, bInitialize)
  if _tBuildingStates[nBuilding] then
    return
  end
  local uBuilding = Pg.GetGuidByName(_tBuildings[nBuilding])
  Debug.Printf("Pmc Building " .. nBuilding .. " (" .. tostring(uBuilding) .. ") has been ressurected!")
  _tBuildingStates[nBuilding] = true
  if not bInitialize then
    Object.Revive(uBuilding)
    if nBuilding == 1 then
      _EnablePortals(true, false)
    elseif nBuilding == 2 then
      _SetPmcTransitLocation(true)
    end
  end
  _tBuildingEvents[nBuilding] = Event.Create(Event.ObjectDeath, {uBuilding}, _OnPmcDeath, {nBuilding})
end

function _SetPmcTransitLocation(bEnable)
  MrxTransit.SuppressLocation(1, not bEnable)
end

function _DoParkingLot(sMissionName)
  local sMissionId = WifMissionFlow.GetCaseSensitiveMissionId(sMissionName)
  local tRewards = MrxRewardData.GetRewards(sMissionId)
  Debug.Printf(" =-= ", sMissionId, " ", type(tRewards))
  if tRewards then
    local tWagerData = MrxRewardData.GetWagerData(tRewards)
    if tWagerData then
      Debug.Printf(" =-= WAGER! ")
      Event.Post("parkingLotStart", {false})
      return
    end
  end
  Debug.Printf(" =-= NOT A WAGER! ")
  local uParkingLotPoint = Pg.GetGuidByName("01_pmc_hq_parking")
  local uHeliPoint = Pg.GetGuidByName("01_pmc_hq_lz_playerone")
  local tPortalData = _tPortalData[_nPortal]
  Event.Post("parkingLotStart", {
    Pg.GetGuidByName(tPortalData.sExterior_Entrance),
    uParkingLotPoint,
    uHeliPoint
  })
end
