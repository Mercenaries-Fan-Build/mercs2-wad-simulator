inherit("MrxTaskContract")
import("MrxSubtitle")
import("DangerousBuilding")
import("MrxLayerManager")
import("MrxVoSequence")
import("MrxState")
import("MrxSupportData")
import("MrxSupportManager")
import("MrxMusic")
import("MrxSupport")
import("WifPmcInterior")
import("MrxSupportTransit")
import("MrxPmc")
import("WifVzBoundary")

function LoadAssets(self, tSaveData)
  local tLayersToRemove = {
    "vz_State_Pmc_LivedIn",
    "vz_state_merida_act2_helo"
  }
  local tLayersToAdd = {
    "vz_state_pmccon003_solbunkerbase",
    "vz_state_pmccon003",
    "vz_state_sol_bunker",
    "vz_state_sol_base_pristine"
  }
  if self:_GetFlag("BunkerBusterDeployed") then
    table.insert(tLayersToRemove, "vz_state_pmccon003_bunkerdefenses")
    table.insert(tLayersToRemove, "vz_state_pmccon003_BunkerAA")
  else
    table.insert(tLayersToAdd, "vz_state_pmccon003_bunkerdefenses")
    table.insert(tLayersToAdd, "vz_state_pmccon003_BunkerAA")
  end
  if self:_GetFlag("PMCRaceComplete") and not self:_GetFlag("PMCDefenseComplete") then
    table.insert(tLayersToAdd, "vz_state_pmccon003_pmcattack")
  else
    table.insert(tLayersToRemove, "vz_state_pmccon003_pmcattack")
  end
  if self:_GetFlag("PMCDefenseComplete") then
    table.insert(tLayersToAdd, "vz_state_pmccon003_getcarmona")
  else
    table.insert(tLayersToRemove, "vz_state_pmccon003_getcarmona")
  end
  MrxLayerManager.Add(tLayersToAdd, function()
    MrxLayerManager.Remove(tLayersToRemove, self.AssetsLoaded, {self})
  end)
end

function Activated(self)
  MrxTaskContract.Activated(self)
  uPMCguid = Pg.GetGuidByName("_Pmcoutpost_bld_hq_livedin_pmccon003 0x000fe1ff")
  oGetCarmona = nil
  uCarmona = nil
  uCarmonaJeep = nil
  uCarmonaHeli = nil
  tEscapePaths = {
    "PTH_Carmona_Flee_01",
    "PTH_Carmona_Flee_02",
    "PTH_Carmona_Flee_03",
    "PTH_Carmona_Flee_04",
    "PTH_Carmona_Flee_05"
  }
  WifVzBoundary.SetupBoundaryPMCCON003()
  Object.SetInvincible(Pg.GetGuidByName("Solano_Bunker"), true)
  MrxSupportManager.IsRecruitAvailable("Copter")
  MrxSupportManager.StartRecruitCooldown("Copter", -1)
  if self:_GetFlag("PMCDefenseComplete") then
    Debug.Printf("************** PMC 003: PMC DEFENSE COMPLETE FLAG RETRIEVED")
    self:GetCarmona(self)
  elseif self:_GetFlag("PMCRaceComplete") then
    Debug.Printf("************** PMC 003: PMC RACE COMPLETE FLAG RETRIEVED")
    self:ClearPMCGrounds(self)
  elseif self:_GetFlag("BunkerBusterDeployed") then
    Debug.Printf("************** PMC 003: Bunker Buster FLAG RETRIEVED")
    Object.Remove(Pg.GetGuidByName("PMC003_EwanTaxi"))
    self:BunkerBuster(self)
  elseif self:_GetFlag("BoardedLuckyLady") then
    Debug.Printf("************** PMC 003: Boarded Lucky Lady FLAG RETRIEVED")
    Object.Remove(Pg.GetGuidByName("PMC003_EwanTaxi"))
    self:BunkerBuster(self)
  else
    local uTaxi = Pg.GetGuidByName("PMC003_EwanTaxi")
    local uDriver = Vehicle.GetDriver(uTaxi)
    self.uIdleGoal = Ai.Goal({
      AIGuid = uDriver,
      Goal = "Idle",
      Priority = "hiPri",
      MaintainRotorSpeed = true
    })
    self:CreateChild({
      sName = "Action",
      sModuleName = "MrxTaskObjectiveEnterVehicle",
      vTgtInclude = uTaxi,
      uPlayer = Player.GetAllCharacters(),
      sActionLabel = "[PmcCon003.Objectives.001]",
      sDspShortDesc = "[PmcCon003.Objectives.001]",
      bUseAnySeat = true,
      tOnComplete = {
        {
          TransitTeleport,
          {self, uTaxi}
        }
      },
      tOnCancel = {
        {
          self.Cancel,
          {self}
        }
      },
      vVoSeqOnAdd = {
        {
          "Ewan-In-Mission-Contract-Pmc03-127",
          uDriver
        },
        {
          "Ewan-In-Mission-Contract-Pmc03-124",
          uDriver
        },
        {
          "Ewan-In-Mission-Contract-Pmc03-125",
          uDriver
        }
      }
    })
  end
end

function TransitTeleport(self, uHeli)
  Debug.Printf("************** TRANSIT TELEPORT EXECUTING!")
  if Vehicle.GetFromRider(Player.GetPrimaryCharacter()) ~= uHeli then
    Vehicle.Enter(uHeli, Player.GetPrimaryCharacter(), "p", true, false)
  end
  MrxSupportTransit:TransitToPoint(uHeli, Pg.GetGuidByName("loc_ChopperDropoff_P1"))
  self:_CreateEvent(Event.ObjectInSeat, {
    "Hero",
    uHeli,
    "a",
    "x"
  }, BunkerBuster, {self})
end

function BunkerBuster(self)
  if not self:IsActive() then
    return
  end
  local t1, t2 = MrxSupportManager.GetRecruitTimes("copter")
  Debug.Printf(" recruit times: ", t1, ", ", t2)
  self:_SetFlag("BoardedLuckyLady")
  if self:_GetFlag("BunkerBusterDeployed") then
    _Checkpoint({
      "loc_HindCheckpoint_P1",
      "loc_HindCheckpoint_P2"
    })
  else
    _Checkpoint({
      "loc_ChopperDropoff_P1",
      "loc_ChopperDropoff_P2"
    })
  end
  Debug.Printf("********************* PMCCON003 Boarded Lucky Lady: FLAG SET")
  oBustBunker = self:CreateChild({
    sName = "Deploy Bunker Buster on Solano's Bunker.",
    sModuleName = "MrxTaskObjectiveDestroy",
    vTgtInclude = {
      "Solano_Bunker"
    },
    sDspShortDesc = "[PmcCon003.Objectives.002]",
    tOnComplete = {
      {
        self.VODeployedBB,
        {self}
      }
    },
    tOnCancel = {
      {
        self.Cancel,
        {self}
      }
    },
    vVoSeqOnAdd = {
      "Fiona-In-Mission-Contract-Pmc03-01",
      "Fiona-In-Mission-Contract-Pmc03-02"
    }
  })
  self:_CreatePersistentEvent(Event.ScriptEvent, {
    "Busted",
    function()
      return true
    end
  }, BBDeployed, {self})
  MrxSupportData.AddFreebie("Bunker Buster", 1, Player.GetPrimaryPlayer())
  BunkerApproachRegionActivate(self)
  BunkerTankAmbushRegionActive(self)
  AngelFallsBridgeRegionActivate(self)
end

function VODeployedBB(self)
  local t1, t2 = MrxSupportManager.GetRecruitTimes("copter")
  Debug.Printf(" recruit times: ", t1, ", ", t2)
  MrxSupportManager.StartRecruitCooldown("Copter", -1)
  self:_SetFlag("BunkerBusterDeployed")
  _Checkpoint({
    "loc_HindCheckpoint_P1",
    "loc_HindCheckpoint_P2"
  })
  Debug.Printf("********************* PMCCON003 Bunker Buster Deployed: FLAG SET")
  MrxVoSequence.Start({
    {
      mattias = "Mattias-In-Mission-Contract-Pmc03-03",
      jennifer = "Jennifer-In-Mission-Contract-Pmc03-04",
      chris = "Chris-In-Mission-Contract-Pmc03-123"
    },
    "Fiona-In-Mission-Contract-Pmc03-134",
    {
      mattias = "Mattias-In-Mission-Contract-Pmc03-82",
      jennifer = "Jennifer-In-Mission-Contract-Pmc03-83",
      chris = "Chris-In-Mission-Contract-Pmc03-84"
    },
    "Fiona-In-Mission-Contract-Pmc03-54",
    {
      mattias = "Mattias-In-Mission-Contract-Pmc03-85",
      jennifer = "Jennifer-In-Mission-Contract-Pmc03-86",
      chris = "Chris-In-Mission-Contract-Pmc03-87"
    },
    "Fiona-In-Mission-Contract-Pmc03-23",
    {
      mattias = "Mattias-In-Mission-Contract-Pmc03-24",
      jennifer = "Jennifer-In-Mission-Contract-Pmc03-25",
      chris = "Chris-In-Mission-Contract-Pmc03-26"
    },
    "Fiona-In-Mission-Contract-Pmc03-56",
    {
      mattias = "Mattias-In-Mission-Contract-Pmc03-88",
      jennifer = "Jennifer-In-Mission-Contract-Pmc03-89",
      chris = "Chris-In-Mission-Contract-Pmc03-90"
    },
    "Fiona-In-Mission-Contract-Pmc03-50",
    {
      mattias = "Mattias-In-Mission-Contract-Pmc03-91",
      jennifer = "Jennifer-In-Mission-Contract-Pmc03-92",
      chris = "Chris-In-Mission-Contract-Pmc03-93"
    },
    {
      self.PMCRace,
      {self}
    }
  })
end

function BBDeployed(self, tLoc)
  local nStock = MrxPmc.GetSupportQty("bunkerbuster")
  local nFree = MrxPmc.GetFreebieQty("[support.airstrike.bunkerbuster.name]")
  nStock = nStock or 0
  nFree = nFree or 0
  Debug.Printf("*************************!!!!!!!!!!**********!!!!!!!!!!*********!!!!!!!!!!Stockpile: " .. nStock .. " , Freebies: " .. nFree)
  if MrxUtil.GetDistanceToObject("Solano_Bunker", tLoc[1], tLoc[2], tLoc[3], true) < 100 then
    Debug.Printf("*********************************************************************** PMCCON003: Completing BB Objective")
    oBustBunker:Complete()
  elseif nFree < 1 and nStock < 1 then
    Debug.Printf("*********************************************************************** PMCCON003: PLAYER FAILED TO TARGET BUNKER WITH BUSTER")
    MrxVoSequence.Start({
      "Fiona-In-Mission-Contract-Pmc03-132",
      3,
      {
        self.FailedToBust,
        {self}
      }
    })
  end
end

function FailedToBust(self)
  self:_SetCancelMessage("[PmcCon003.Terms.Cancel01]")
  self:Cancel()
end

function PMCRace(self)
  Debug.Printf("$$$$$$$$$$$!!!!!!!!!!!!!!  PMC RACE BEGINS!")
  self:CreateChild({
    sName = "Race back to the base in time!",
    sModuleName = "MrxTaskObjectiveDeliver",
    vDestRegion = "Region_PMC003_MansionApproach",
    vTgtInclude = Player.GetAnyCharacter(),
    bDspBlp = true,
    nTimeLimit = 600,
    sDspShortDesc = "[PmcCon003.Objectives.003]",
    bStop = false,
    bXZOnly = false,
    vVoSeqOnAdd = {
      "Fiona-In-Mission-Contract-Pmc03-59"
    },
    tOnComplete = {
      {
        self.LoadPMCAttack01,
        {self}
      },
      {
        Pg.StopHeliWaveSpawner,
        {self}
      }
    },
    fOnCancel = function()
      nHealth = Object.GetHealth(Player.GetLocalCharacter())
      Debug.Printf("********************* Objective Cancelled - Time UP!")
      if nHealth > 0 then
        Debug.Printf("*********************Time ran out!")
        self:_SetCancelMessage("[PmcCon003.Terms.Cancel02]")
        local tSequence = {
          "Fiona-In-Mission-Contract-PMC03-148",
          {
            self.Cancel,
            {self}
          }
        }
        MrxVoSequence.Start(tSequence)
      end
    end
  })
  StartHeliPursuit(self)
  MrxMusic.PlaySpecialMusic("mu_pmc_panicloop_01")
  local uPMCLocGuid = Pg.GetGuidByName("LOC_PMC")
  self:_CreateEvent(Event.ObjectProximity, {
    Player.GetAnyCharacter(),
    uPMCLocGuid,
    "<",
    5000,
    false,
    true
  }, MrxVoSequence.Start, {
    {
      "Fiona-In-Mission-Contract-PMC03-141"
    }
  })
  self:_CreateEvent(Event.ObjectProximity, {
    Player.GetAnyCharacter(),
    uPMCLocGuid,
    "<",
    3750,
    false,
    true
  }, MrxVoSequence.Start, {
    {
      "Fiona-In-Mission-Contract-PMC03-143"
    }
  })
  self:_CreateEvent(Event.ObjectProximity, {
    Player.GetAnyCharacter(),
    uPMCLocGuid,
    "<",
    2500,
    false,
    true
  }, MrxVoSequence.Start, {
    {
      "Fiona-In-Mission-Contract-PMC03-142"
    }
  })
  self:_CreateEvent(Event.ObjectProximity, {
    Player.GetAnyCharacter(),
    uPMCLocGuid,
    "<",
    1250,
    false,
    true
  }, MrxVoSequence.Start, {
    {
      "Fiona-In-Mission-Contract-PMC03-147"
    }
  })
end

function LoadPMCAttack01(self)
  MrxLayerManager.Add({
    "vz_state_pmccon003_pmcattack"
  }, ClearPMCGrounds, {self})
end

function ClearPMCGrounds(self)
  self:_SetFlag("PMCRaceComplete")
  _Checkpoint({
    "loc_DefendPMCCheckpoint_P1",
    "loc_DefendPMCCheckpoint_P2"
  })
  Debug.Printf("********************* PMCCON003 PMC Race Complete : FLAG SET")
  Object.Remove(Pg.GetGuidByName("PMC003_EwanTaxi"))
  WifPmcInterior.SetEntranceLock(true)
  WifPmcInterior.RefreshUiDisplay()
  tInitialVOTable = {
    "Fiona-In-Mission-Contract-Pmc03-149",
    "Fiona-In-Mission-Contract-Pmc03-150"
  }
  self:_PMCHealthBar()
  oDestroyAttackers = self:CreateChild({
    sName = "Clear out the VZ forces attacking the PMC!",
    sModuleName = "MrxTaskObjectiveDestroy",
    vTgtInclude = {
      "PMC003_AMX_01",
      "PMC003_AMX_02",
      "PMC003_AMX_03",
      "PMC003_AMX_04"
    },
    bDspBlp = true,
    sDspShortDesc = "[PmcCon003.Objectives.004]",
    fOnComplete = function()
      if Object.IsAlive(Pg.GetGuidByName("OBJ_Wave02_Tank01")) or Object.IsAlive(Pg.GetGuidByName("OBJ_Wave02_Tank02")) or Object.IsAlive(Pg.GetGuidByName("OBJ_Wave02_Tank03")) or Object.IsAlive(Pg.GetGuidByName("OBJ_Wave02_Tank04")) then
        ClearPMCGrounds_Wave2(self)
        Debug.Printf("********************* PMC DEAD!!!")
      else
        EnterPMC(self)
        MrxUtil.StopHealthBar(uPMCguid)
      end
    end,
    fOnCancel = function()
      MrxUtil.StopHealthBar(uPMCguid)
      self:_SetCancelMessage("[PmcCon003.Terms.Cancel03]")
      self.Cancel(self)
    end,
    vVoSeqOnAdd = tInitialVOTable
  })
  self:_CreateEvent(Event.ObjectHealthLessThan, {uPMCguid, 1}, function()
    Debug.Printf("********************* PMC DEAD!!!")
    oDestroyAttackers.Cancel(oDestroyAttackers)
  end)
  local TankGuid1 = Pg.GetGuidByName("PMC003_AMX_01")
  local TankGuid2 = Pg.GetGuidByName("PMC003_AMX_02")
  local TankGuid3 = Pg.GetGuidByName("PMC003_AMX_03")
  local TankGuid4 = Pg.GetGuidByName("PMC003_AMX_04")
  TankAttackPMC(self, TankGuid1)
  TankAttackPMC(self, TankGuid2)
  TankAttackPMC(self, TankGuid3)
  TankAttackPMC(self, TankGuid4)
end

function LoadPMCAttack02(self)
end

function ClearPMCGrounds_Wave2(self)
  local tInitialVOTable = {
    "Fiona-In-Mission-Contract-Pmc03-151"
  }
  oDestroyAttackersWave2 = self:CreateChild({
    sName = "Clear out the VZ forces attacking the PMC!",
    sModuleName = "MrxTaskObjectiveDestroy",
    vTgtInclude = {
      "OBJ_Wave02_Tank01",
      "OBJ_Wave02_Tank02",
      "OBJ_Wave02_Tank03",
      "OBJ_Wave02_Tank04"
    },
    bDspBlp = true,
    sDspShortDesc = "[PmcCon003.Objectives.004]",
    tOnComplete = {
      {
        self.EnterPMC,
        {self}
      },
      {
        MrxUtil.StopHealthBar,
        {uPMCguid}
      }
    },
    vVoSeqOnAdd = tInitialVOTable
  })
  if Object.IsAlive(Pg.GetGuidByName("OBJ_Wave02_Tank03")) then
    TankBustPMCWall(self, "OBJ_Wave02_Tank03", "_pmcoutpost_walla 0x000a3e1c", "Path_EastArmor01")
  end
  if Object.IsAlive(Pg.GetGuidByName("OBJ_Wave02_APC02")) then
    TankBustPMCWall(self, "OBJ_Wave02_APC02", "_pmcoutpost_walla 0x000a3e1d", "Path_EastArmor02")
  end
  if Object.IsAlive(Pg.GetGuidByName("OBJ_Wave02_Tank04")) then
    TankBustPMCWall(self, "OBJ_Wave02_Tank04", "_pmcoutpost_walla 0x000a3e1e", "Path_EastArmor03")
  end
  if Object.IsAlive(Pg.GetGuidByName("OBJ_Wave02_Tank02")) then
    TankBustPMCWall(self, "OBJ_Wave02_Tank02", "_pmcoutpost_walla 0x000a3dea", "PTH_West_Attack_01")
  end
  if Object.IsAlive(Pg.GetGuidByName("OBJ_Wave02_APC01")) then
    TankBustPMCWall(self, "OBJ_Wave02_APC01", "_pmcoutpost_wallb 0x000a3de8", "PTH_West_Attack_02")
  end
  if Object.IsAlive(Pg.GetGuidByName("OBJ_Wave02_Tank01")) then
    TankBustPMCWall(self, "OBJ_Wave02_Tank01", "_pmcoutpost_walla 0x000a3deb", "PTH_West_Attack_03")
  end
end

function TankAttackPMC(self, uTankGuid)
  Ai.RemoveGoal({AIGuid = uTankGuid, Handle = 0})
  tAttackGoalParams = {
    AIGuid = Vehicle.GetDriver(uTankGuid),
    Goal = "Attack",
    Target = uPMCguid,
    Priority = "MedPri",
    Force = true
  }
  Debug.Printf("********************* STARTING TANK ATTACK")
  self:_CreateEvent(Event.ObjectHibernation, {uTankGuid, "awake"}, Ai.Goal, {tAttackGoalParams})
end

function TankBustPMCWall(self, sTankName, sWallName, sPathName)
  Debug.Printf("********************* BUSTING THE PMC WALL")
  local uPos = Pg.GetGuidByName(sWallName)
  local x, y, z = Object.GetPosition(uPos)
  Pg.Spawn("Explosion (Rocket Artillery)", x, y, z, 0, false, true)
  local uTankGuid = Pg.GetGuidByName(sTankName)
  Ai.RemoveGoal({AIGuid = uTankGuid, Handle = 0})
  Ai.Goal({
    AIGuid = Vehicle.GetDriver(uTankGuid),
    Goal = "PathMove",
    Target = Pg.GetGuidByName(sPathName),
    Priority = "HiPri",
    Haste = 1,
    Force = true,
    Callback = TankAttackPMC,
    CallbackData = {self, uTankGuid}
  })
end

function _PMCHealthBar(self)
  self:_CreateEvent(Event.Boundary, {
    Player.GetAnyCharacter(),
    GetGuidByName("Region_PMC003_PMC"),
    "enter"
  }, function()
    MrxUtil.DisplayHealthBar(self, uPMCguid, 0, true, 0)
    self:_CreateEvent(Event.Boundary, {
      Player.GetAnyCharacter(),
      GetGuidByName("Region_PMC003_MansionApproach"),
      "exit"
    }, function()
      MrxUtil.StopHealthBar(uPMCguid)
      _PMCHealthBar(self)
    end)
  end)
end

function EnterPMC(self)
  self:CreateChild({
    sName = "Get inside the PMC!",
    sModuleName = "MrxTaskObjectiveAction",
    sActionLabel = "[ContextAction.Enter]",
    sDspShortDesc = "[PmcCon003.Objectives.006]",
    vTgtInclude = "PlayerLocation_PMC",
    bDspBlp = true,
    tOnComplete = {
      {
        self.CinematicCarmonaPMC,
        {self}
      }
    },
    tOnCancel = {
      {
        self.Cancel,
        {self}
      }
    },
    vVoSeqOnAdd = {
      "Fiona-In-Mission-Contract-Pmc03-152"
    }
  })
end

function CinematicCarmonaPMC(self)
  local function _MoviePlayed()
    MrxState.Exit(MrxState.STATE_WAITFORGAME, self.SpawnCarmona, {self})
  end
  
  local function _PlayMovie()
    local sHeroLetter = MrxUtil.GetCharacterIdentity(Player.GetPrimaryCharacter())
    sHeroLetter = sHeroLetter and string.upper(string.sub(sHeroLetter, 1, 1))
    if sHeroLetter ~= "M" and sHeroLetter ~= "J" and sHeroLetter ~= "C" then
      sHeroLetter = "M"
    end
    Hud.Cinematic:Show({
      sMovie = "12_CAR_" .. sHeroLetter,
      fCallback = _MoviePlayed,
      bSubtitles = true
    })
  end
  
  MrxLayerManager.Remove("vz_state_pmccon003_pmcattack")
  MrxState.Enter(MrxState.STATE_WAITFORGAME, _PlayMovie)
end

function SpawnCarmona(self)
  MrxLayerManager.Add({
    "vz_state_pmccon003_getcarmona"
  }, GetCarmona, {self})
end

function GetCarmona(self)
  self:_SetFlag("PMCDefenseComplete")
  _Checkpoint({
    "loc_CheckPointGetCarmona_P1",
    "loc_CheckPointGetCarmona_P2"
  })
  Debug.Printf("********************* PMCCON003 PMC DEFENSE Complete : FLAG SET")
  Object.Remove(Pg.GetGuidByName("PMC003_EwanTaxi"))
  MrxSupportManager.MakeRecruitAvailable("Copter")
  uCarmona = Pg.GetGuidByName("CarmonaTarget")
  uCarmonaJeep = Pg.GetGuidByName("CarmonaJeep")
  uCarmonaHeli = Pg.GetGuidByName("CarmonaHeli")
  oGetCarmona = self:CreateChild({
    sName = "Verify Carmona",
    sModuleName = "MrxTaskObjectiveVerify",
    vTgtInclude = {
      "CarmonaTarget"
    },
    bDspBlp = true,
    sDspShortDesc = "[PmcCon003.Objectives.005]",
    fOnComplete = function()
      local tSequence = {
        "Fiona-In-Mission-Contract-Pmc03-64",
        {
          self.Complete,
          {self}
        }
      }
      MrxVoSequence.Start(tSequence)
    end,
    fOnCancel = function()
      local tSequence = {
        "Fiona-In-Mission-Contract-Pmc03-158",
        {
          self.Cancel,
          {self}
        }
      }
      MrxVoSequence.Start(tSequence)
    end,
    vVoSeqOnAdd = {
      "Fiona-In-Mission-Contract-Pmc03-154"
    },
    sFactionId = "Oil"
  })
  MountUpCarmona(self)
end

function MountUpCarmona(self)
  Ai.SetState({
    AIGuid = uCarmona,
    State = "Vip",
    Value = true
  })
  local tGoalParams = {
    AIGuid = uCarmona,
    Goal = "Enter",
    Target = uCarmonaJeep,
    Role = "driver",
    Haste = 1.5,
    Force = true,
    Priority = "hiPri",
    Callback = DriveCarmonaToHeli,
    CallbackData = {self, 0}
  }
  self:_CreateEvent(Event.TimerRelative, {1}, function()
    local r = Ai.Goal(tGoalParams)
    Debug.Printf("********************* CARMONA IS GOING TO THE JEEP: ", r)
  end)
  self:_CreateEvent(Event.ObjectInSeat, {
    uCarmona,
    uCarmonaJeep,
    "D",
    "E"
  }, function()
    StartPursuit(self)
    HeliWaitForCarmona(self)
  end)
end

function DriveCarmonaToHeli(self, step, guid, state)
  local tSize = #tEscapePaths
  Debug.Printf("********************* TABLE SIZE IS ", tSize)
  Debug.Printf("********************* CURRENT STEP IS ", step)
  Debug.Printf("********************* CURRENT STATE IS ", state)
  if state == 1 then
    if step == 0 then
      Debug.Printf("********************* CARMONA IS IN THE JEEP")
      Debug.Printf("********************* CARMONA MIGHT MOVE ALONG FIRST SEGMENT")
    end
    step = step + 1
    if eventHandle then
      Event.Delete(eventHandle)
      eventHandle = nil
    end
  else
    Debug.Printf("********************* FAILED !!! CARMONA MIGHT MOVE ALONG FIRST SEGMENT")
    Debug.Printf("********************* CURRENT PATH INDEX IS ", step)
  end
  if tSize < step then
    Debug.Printf("********************* CARMONA FINISHED ALL PATHS")
    SwitchCarmonaToHeli()
    if eventHandle then
      Event.Delete(eventHandle)
      eventHandle = nil
    end
  else
    Debug.Printf("********************* CURRENT PATH INDEX IS ", step)
    Debug.Printf("********************* CURRENT PATH NAME IS ", tEscapePaths[step])
    local uPathGuid = Pg.GetGuidByName(tEscapePaths[step])
    local uHaste = math.randf(0.5, 0.7)
    Debug.Printf("********************* CURRENT PATH GUID IS ", uPathGuid)
    local tGoalParams = {
      AIGuid = Vehicle.GetDriver(uCarmonaJeep),
      Goal = "PathMove",
      Target = uPathGuid,
      Priority = "HiPri",
      Force = true,
      Haste = uHaste,
      Timeout = 0,
      Start = "Nearest",
      Callback = DriveCarmonaToHeli,
      CallbackData = {self, step}
    }
    local r = Ai.Goal(tGoalParams)
    Debug.Printf("********************* DID AI GOAL SUBMIT SUCCESSFULLY? ", r, " step: ", step)
    eventHandle = self:_CreateEvent(Event.TimerRelative, {15}, DriveCarmonaTimeOut, {
      self,
      step,
      Vehicle.GetDriver(uCarmonaJeep)
    })
  end
end

function DriveCarmonaTimeOut(self, step, guid)
  if eventHandle then
    Event.Delete(eventHandle)
    eventHandle = nil
  end
  Debug.Printf("********************* AI GOAL FAILED - TIMEOUT EVENT ", type(self), step, guid)
  DriveCarmonaToHeli(self, step, guid, 0)
end

function DriveCarmonaDrive(self)
  Debug.Printf("********************* CARMONA IS DRIVING ")
  Ai.Goal({
    AIGuid = Vehicle.GetDriver(uCarmonaJeep),
    Goal = "PathMove",
    Target = Pg.GetGuidByName("Path_PMC003_CarmonaFlee"),
    Priority = "HiPri",
    Force = true,
    Haste = 0.75,
    Timeout = 0,
    Callback = SwitchCarmonaToHeli,
    CallbackData = {}
  })
  StartPursuit(self)
  HeliWaitForCarmona(self)
end

function ExitAttempted(uCarmona, uCarmonaHeli, guid, state)
  Debug.Printf("AI ", guid)
  Debug.Printf("State", state)
  if state == 1 then
    Debug.Printf("********************* CARMONA GOT OUT OF THE JEEP ")
    Debug.Printf("********************* CARMONA ASKED TO GET IN HELICOPTER ")
    Debug.Printf("********************* CARMONA", uCarmona)
    Debug.Printf("********************* HELICOPTER ", uCarmonaHeli)
    local tGoalParams = {
      AIGuid = uCarmona,
      Goal = "Enter",
      Target = uCarmonaHeli,
      Role = "driver",
      Force = true,
      Priority = "hiPri"
    }
    Ai.Goal(tGoalParams)
  else
    Ai.Goal({
      AIGuid = uCarmona,
      Goal = "Exit",
      Priority = "hiPri",
      Callback = ExitAttempted
    })
  end
end

function CarmonaStoppedAtDestn(uCarmona, uCarmonaHeli, guid, state)
  if state == 1 then
    Debug.Printf("********************* CARMONAS JEEP STOPPED ")
    Debug.Printf("********************* CARMONA ASKED TO GET OUT OF JEEP ")
    Ai.RemoveGoal({AIGuid = uCarmona, Handle = 0})
    Ai.Goal({
      AIGuid = uCarmona,
      Goal = "Exit",
      Priority = "hiPri",
      Callback = ExitAttempted,
      CallbackData = {uCarmona, uCarmonaHeli}
    })
  else
    Ai.Goal({
      AIGuid = uCarmona,
      Goal = "Stop",
      Priority = "hiPri",
      Callback = CarmonaStoppedAtDestn,
      CallbackData = {uCarmona, uCarmonaHeli}
    })
  end
end

function SwitchCarmonaToHeli(self)
  Debug.Printf("********************* CARMONA GETTING OUT OF THE JEEP ")
  StopPursuit(self)
  Ai.RemoveGoal({AIGuid = uCarmona, Handle = 0})
  Ai.Goal({
    AIGuid = uCarmona,
    Goal = "Stop",
    Priority = "hiPri",
    Callback = CarmonaStoppedAtDestn,
    CallbackData = {uCarmona, uCarmonaHeli}
  })
end

function HeliWaitForCarmona(self)
  Debug.Printf("********************* POLLING FOR CARMONA HELICOPTER ENTRY ")
  self:_CreateEvent(Event.ObjectInSeat, {
    uCarmona,
    uCarmonaHeli,
    "D",
    "E"
  }, CarmonaHeliEscape, {self})
end

function CarmonaHeliEscape(self)
  Debug.Printf("********************* CARMONA GOT IN THE HELICOPTER: ")
  Ai.RemoveGoal({AIGuid = uCarmona, Handle = 0})
  MrxSupport.GoHome(self, uCarmonaHeli)
  self:_CreateEvent(Event.ObjectHibernation, {uCarmonaHeli, "hibernated"}, oGetCarmona.Cancel, {oGetCarmona})
end

function BunkerApproachRegionActivate(self)
  self:_CreateEvent(Event.Boundary, {
    Player.GetAnyCharacter(),
    Pg.GetGuidByName("Region_PmcCon003_Bunker_ApproachEnc01"),
    "enter",
    false
  }, Bunker_Approach_Attack01, {self})
  Debug.Printf("********************* SOLANO BUNKER APPROACH REGION ACTIVE ")
end

function Bunker_Approach_Attack01(self)
  local uChopper = Pg.GetGuidByName("Chopper_PmcCon003_Bunker_ApproachEnc01")
  if uChopper then
    Ai.Goal({
      AIGuid = Vehicle.GetDriver(uChopper),
      Goal = "PathMove",
      Target = Pg.GetGuidByName("Path_PmcCon003_Bunker_ApproachEnc01"),
      Priority = "LowPri",
      Haste = 0.5
    })
  else
    Debug.Printf("ERROR: Unable to find Chopper_PmcCon003_Bunker_ApproachEnc01")
  end
end

function BunkerTankAmbushRegionActive(self)
  self:_CreateEvent(Event.Boundary, {
    Player.GetAnyCharacter(),
    Pg.GetGuidByName("Region_Pmc003_Bunker_TankAmbush01"),
    "enter",
    false
  }, BunkerTankAmbush01, {self})
  Debug.Printf("********************* SOLANO BUNKER TANK AMBUSH REGION ACTIVE ")
end

function BunkerTankAmbush01(self)
  Debug.Printf("********************* SOLANO BUNKER TANK AMBUSH REGION SUCCESS ")
  local uTank = Pg.GetGuidByName("Scorpion_TankAmbush01")
  if uTank then
    Ai.Goal({
      AIGuid = Vehicle.GetDriver(uTank),
      Goal = "PathMove",
      Target = Pg.GetGuidByName("Path_Pmc003_Bunker_TankAmbush01"),
      Priority = "MedPri",
      Haste = 1
    })
  else
    Debug.Printf("ERROR: Unable to find Scorpion_TankAmbush01")
  end
end

function AngelFallsBridgeRegionActivate(self)
  uEvent = self:_CreateEvent(Event.Boundary, {
    Player.GetAnyCharacter(),
    Pg.GetGuidByName("region_pmccon003_angelfallsbridge"),
    "enter",
    false
  }, AngelFallsBridgeAmbush, {self})
  Debug.Printf("Region Guid: " .. tostring(Pg.GetGuidByName("Region_PmcCon003_AngelFallsBridge")))
  Debug.Printf(tostring(uEvent), "***********UEVENT TRIGGERED")
  Debug.Printf("********************* SOLANO BUNKER BRIDGE AMBUSH REGION ACTIVE ")
end

function AngelFallsBridgeAmbush(self)
  local uHeli = Pg.GetGuidByName("PmcCon003_Heli_BridgeAmbush")
  if uHeli then
    Debug.Printf("******** Angel Falls Bridge Ambush found uHeli - giving AI goal!!")
    Ai.Goal({
      AIGuid = Vehicle.GetDriver(uHeli),
      Goal = "PathMove",
      Target = Pg.GetGuidByName("Path_PmcCon003_AngelFallsBridge"),
      Priority = "LowPri",
      Haste = 1
    })
  else
    Debug.Printf("ERROR: Unable to find PmcCon003_Heli_BridgeAmbush")
  end
end

function StartPursuit(self)
  self:_CreateEvent(Event.TimerRelative, {10})
  local tPursuitTable = {
    {
      "Driving",
      {
        {
          "Car",
          "M151 .50Cal (VZ) (DriverGunner)",
          1
        },
        {
          "Tank",
          "Scorpion90 (Full)",
          1
        }
      },
      {
        {"Car", 4},
        {"Tank", 2}
      }
    },
    {
      "Stopped",
      {
        {
          "Car",
          "M151 .50Cal (VZ) (DriverGunner)",
          1
        },
        {
          "Tank",
          "Scorpion90 (Full)",
          1
        }
      },
      {
        {"Car", 4},
        {"Tank", 2}
      }
    },
    {
      "Offroad",
      {
        {
          "Car",
          "M151 .50Cal (VZ) (DriverGunner)",
          1
        },
        {
          "Tank",
          "Scorpion90 (Full)",
          1
        }
      },
      {
        {"Car", 4},
        {"Tank", 2}
      }
    }
  }
  MrxFactionManager.SetCustomPursuit(Pg.GetGuidByName("VZ"), -1, tPursuitTable)
end

function StopPursuit(self)
  Debug.Printf("SSSSSSSSSSSSSSSStop stop Pursuit ")
  MrxFactionManager.ClearCustomPursuit()
end

function StartHeliPursuit(self)
  local heli1 = {
    Pg.GetGuidByName("Alouette3 Elite (Driver)"),
    1
  }
  local heli2 = {
    Pg.GetGuidByName("Mi35 (AA Driver)"),
    1
  }
  local tWave1 = {
    NumToSpawn = 1,
    WaveDelay = 0,
    SpawnDist = 200,
    units = {heli2}
  }
  local tWave2 = {
    NumToSpawn = 1,
    WaveDelay = 8,
    SpawnDist = 200,
    units = {heli1}
  }
  local tWave3 = {
    NumToSpawn = 1,
    WaveDelay = 15,
    SpawnDist = 300,
    units = {heli1}
  }
  local tWave4 = {
    NumToSpawn = 2,
    WaveDelay = 15,
    SpawnDist = 300,
    units = {heli1}
  }
  local tWave5 = {
    NumToSpawn = 1,
    WaveDelay = 20,
    SpawnDist = 300,
    units = {heli2}
  }
  local tWave6 = {
    NumToSpawn = 1,
    WaveDelay = 15,
    SpawnDist = 300,
    units = {heli1}
  }
  local tWave7 = {
    NumToSpawn = 2,
    WaveDelay = 15,
    SpawnDist = 300,
    units = {heli1}
  }
  Debug.Printf("$$$$$$$$$$$!!!!!!!!!!!!!!  HELI SPAWNER STARTS HERE  !!!!!!!!!!!!!!!!!!!$$$$$$$$$$$$$$$$$")
  Pg.StartHeliWaveSpawner({
    tWave1,
    tWave2,
    tWave3,
    tWave4,
    tWave5,
    tWave6
  })
end

function Cleanup(self)
  StopPursuit(self)
  Pg.StopHeliWaveSpawner(self)
  MrxSupportManager.MakeRecruitAvailable("Copter")
  MrxSupportData.RemoveFreebie("Bunker Buster")
  Object.Remove(Pg.GetGuidByName("PMC003_EwanTaxi"))
  Debug.Printf("....................CLEANUP EXECUTED!")
  MrxUtil.StopHealthBar(uPMCguid)
  WifPmcInterior.SetEntranceLock(false)
  WifPmcInterior.RefreshUiDisplay()
  local tLayersMarkForRemove = {
    "vz_state_pmccon003_BunkerAA",
    "vz_state_pmccon003_bunkerdefenses",
    "vz_state_pmccon003_solbunkerbase",
    "vz_state_pmccon003",
    "vz_state_pmccon003_getcarmona",
    "vz_state_pmccon003_pmcattack"
  }
  MrxLayerManager.MarkForRemoval(tLayersMarkForRemove)
  local tLayersToAdd = {
    "vz_State_Pmc_LivedIn",
    "vz_state_merida_act2_helo"
  }
  MrxLayerManager.MarkForAddition(tLayersToAdd)
  MrxTaskContract.Cleanup(self)
end
