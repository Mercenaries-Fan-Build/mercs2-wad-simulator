import("Munitions", false)
import("MrxBootstrap", false)
import("MrxCheatBootstrap", false)
import("MrxSoundBootstrap", false)
import("MrxLayerManager", false)
import("MrxPlayer", false)
import("MrxPlayState", false)
import("MrxPmc", false)
import("MrxSupportData", false)
import("mrxcratedelivery", false)
import("MrxState", false)
import("MrxTask", false)
import("WifMissionFlow", false)
import("WifMissionData", false)
import("Hero", false)
import("MrxGui", false)
import("MrxUtil", false)
import("MrxMusic", false)
import("MrxBriefing", false)
import("MrxVoSequence", false)
import("DLC_MrxGreenGoblinBomb", true)
_G.g_bIsDlc = true
tDLCMissionData = {
  DlcTestCon01 = {
    sModuleName = "DlcTestCon01",
    sFactionId = "All",
    sStarter = "blah",
    bCriticalPathMission = true
  }
}
tDLCCharacterMap = {
  {
    base = "DLC_Hero",
    templates = {
      "chrisupgrade1",
      "chrisupgrade2",
      "chrisupgrade3"
    },
    models = {
      "DLC_pmc_hum_chris"
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
      "pmc_hum_jen_chickensuit",
      "pmc_hum_jen_v2",
      "pmc_hum_jen_v3",
      "pmc_hum_jen_v4"
    }
  },
  {
    base = "mattias",
    templates = {
      "mattiasupgrade1",
      "mattiasupgrade2",
      "mattiasupgrade3"
    },
    models = {
      "pmc_hum_mattias_chickensuit",
      "pmc_hum_mattias_v2",
      "pmc_hum_mattias_v3"
    }
  }
}
tDlcSupportData = {}
oSupport = mrxcratedelivery:Create()
oSupport:SetCargo("DLC_M1A1")
oSupport:SetDeliveryVehicle("Mi26 (PMC) (Driver)")
tDlcSupportData.dlcm1a1 = {
  sName = "[vehicle.m1a1]",
  sDescription = "[support.vehicle.m1a1.desc]",
  sIcon = "vehicles_tank_m1a2",
  nMaxStock = 4,
  nCashCost = 250000,
  nFuelCost = 75,
  oSupport = oSupport,
  sType = "Heavy"
}
oSupport = DLC_MrxGreenGoblinBomb:Create()
tDlcSupportData.greengoblinbomb = {
  sName = "[support.airstrike.greengoblinbomb.name]",
  sDescription = "[support.airstrike.greengoblinbomb.desc]",
  sIcon = "support_cluster_bomb",
  nMaxStock = 8,
  nCashCost = 20000,
  nFuelCost = 100,
  oSupport = oSupport,
  sType = "Airstrike"
}
tCinematicTimeTable = {}
tCinematicTimeTable[1] = {
  sName = "M2_Attract_Mode-PS2_v6",
  nBegin = 10,
  nEnd = 10,
  sType = "movie"
}
tCinematicTimeTable[2] = {
  sName = "DLCTestCon01.gfx",
  nBegin = 0,
  nEnd = 10,
  sType = "scaleform"
}
tCinematicTimeTable[3] = {
  sName = "SetupDLCMissions",
  nBegin = 11,
  nEnd = 11,
  sType = "missionfunction"
}
tCinematicTimeTable[4] = {
  sName = "Fiona-Banter-Contract-All01-01",
  sMattiasVO = "mattias-Banter-Contract-All01-28",
  sChrisVO = "jennifer-Banter-Contract-All01-30",
  sJenVO = "chris-Banter-Contract-All01-29",
  nBegin = 11,
  nEnd = 11.5,
  sType = "vosequence"
}

function Init()
  _bBootstrapComplete = nil
  _oMasterStub = nil
  MrxLayerManager.ResetState()
  if Net.IsServer() then
    Debug.Printf("DLC: Setup DLC Mission tables and Support")
    WifMissionData.SetMissionData(tDLCMissionData)
    MrxPlayer.SetCharacterMap(tDLCCharacterMap)
    MrxSupportData.AddSupportData(tDlcSupportData.dlcm1a1, "dlcm1a1")
    MrxSupportData.AddSupportData(tDlcSupportData.greengoblinbomb, "greengoblinbomb")
  end
  Pg.LoadAsset("dlc_global_exp_huge", "effect")
  MrxBootstrap.Start(_AttemptGameplaySetup, {"boot"})
  Sys.AddStringDb("dlctest01")
  LoadSingleton(nil)
end

function Deinit()
  Sys.ClearStringDb("dlctest01")
  Pg.UnloadAsset("dlc_global_exp_huge", "effect")
end

function _AttemptGameplaySetup(sSignal)
  if not Net.IsMultiplayer() or Net.IsServer() then
    MrxState.Enter(MrxState.STATE_WAITFORGAME, _InitialStreamComplete)
  end
end

function _InitialStreamComplete()
  local tPlayers = Player.GetAllPlayers()
  for i, uPlayerGuid in ipairs(tPlayers) do
    local uCharacter = Player.GetCharacter(uPlayerGuid)
    Hero.EndSurvivalMode(uPlayerGuid, uCharacter, 0.1)
    Human.Inventory.ReloadAll(uCharacter, false)
    Human.SetState(uCharacter, "Upright", "Idle")
  end
  local tSupportData = MrxSupportData.tSupportData
  for sKey, tData in pairs(tSupportData) do
    MrxPmc.AddSupportQty(sKey, tData.nMaxStock - (MrxPmc.GetSupportQty(sKey) or 0))
  end
  for i, tData in ipairs(tCinematicTimeTable) do
    if tData.sType == "movie" then
      Event.Create(Event.TimerRelative, {
        tData.nBegin
      }, PlayDLCMovie, {
        {
          sMovie = tData.sName,
          fCallback = nil,
          tCallBackData = nil
        }
      })
    elseif tData.sType == "scaleform" then
      MrxBriefing._AddFlashObject(tData.sName, nil)
      Event.Create(Event.TimerRelative, {
        tData.nBegin
      }, MrxBriefing._ShowFlashObject, {
        tData.sName
      })
      Event.Create(Event.TimerRelative, {
        tData.nEnd
      }, MrxBriefing._RemoveFlashObject, {
        tData.sName,
        nil
      })
    elseif tData.sType == "missionfunction" then
      Event.Create(Event.TimerRelative, {
        tData.nBegin
      }, SetupDLCMissions, {})
    elseif tData.sType == "vosequence" then
      Event.Create(Event.TimerRelative, {
        tData.nBegin
      }, PlayVO, {
        tData.sName,
        tData.nEnd - tData.nBegin,
        tData.sMattiasVO,
        tData.sChrisVO,
        tData.sJenVO
      })
    end
  end
  MrxMusic.EnterFreeplayMusic()
  MrxState.Exit(MrxState.STATE_WAITFORGAME)
end

function LoadSingleton(tSaveData)
end

function ResetSingleton()
  local function _PostDeleteLayers(nId)
    Pg.ResetSingletonDone()
  end
  
  local tPlayers = Player.GetAllPlayers()
  for i, uPlayerGuid in ipairs(tPlayers) do
    local uCharacter = Player.GetCharacter(uPlayerGuid)
    Human.ForceExitSeatNoSnap(uCharacter)
  end
  local sLayerName = "dlctest01_base"
  Pg.UnloadingStaticLayers(true)
  local bSuccess = Pg.UnloadLayer(sLayerName)
  Pg.UnloadingStaticLayers(false)
  ASSERT(bSuccess, "Unable to unload layer " .. sLayerName)
  MrxGui:CleanupFadeFlash()
  Pg.ResetSingletonDone()
  MrxSoundBootstrap.UnloadBanks()
end

function SetupDLCMissions()
  if Net.IsServer() then
    Debug.Printf("DLC: SetupDLCMissions function called")
    if _oMasterStub then
      _oMasterStub:Cleanup()
    end
    _oMasterStub = MrxTask:Create()
    _oMasterStub:Configure({
      sName = "DlcTestCon01"
    })
    _oMasterStub:Activate()
    MrxCheatBootstrap.SetTaskTreeRoot(_oMasterStub)
    local oStub = MrxTask:Create()
    oStub:Configure({
      sName = "DlcTestCon01",
      sModuleName = "DlcTestCon01",
      sFactionId = "Pmc",
      oParent = _oMasterStub
    })
    oStub:Activate()
  end
end

function PlayDLCMovie(tArgs)
  Hud.Cinematic:Show({
    sMovie = tArgs.sMovie,
    fCallback = tArgs.fCallback
  })
end

function PlayVO(sName, nLength, sMattiasVO, sChrisVO, sJenVO)
  MrxVoSequence.Start({
    sName,
    nLength,
    {
      mattias = sMattiasVO,
      jennifer = sChrisVO,
      chris = sJenVO
    }
  })
end
