inherit("MrxTaskContract")
import("MrxSubtitle")
import("MrxVoSequence")
import("MrxFactionManager")
import("MrxCinematic")
import("MrxTutorialManager")
import("MrxAchievements")
import("MrxMusic")
ksBarExitBuilding = "_outskirt_bld_mercbar 0x000c7042"
ksBarExitCameraLocation = "BarExit Camera Location"
tPmcDoors = {
  "pmcoutpost_hq_door_roof",
  "pmcoutpost_hq_door_garage",
  "pmcoutpost_hq_door_entrance",
  "pmcoutpost_hqgarage_door_big01",
  "pmcoutpost_hqgarage_door_big02",
  "pmcoutpost_hqgarage_door_backdoor",
  "pmcoutpost_hqgarage_door_topdoor"
}
NETEVENT_SETSTARTUPWEAPONS = 0
NETEVENT_MOVECOLLISION = 1

function ClientMoveCollision()
  local doorGuid = Pg.GetGuidByName("PMC001_FrontDoor_InvisiblePhysics")
  if doorGuid then
    Debug.Printf("Creating invisible physics hibernation event")
    Event.Create(Event.ObjectHibernation, {doorGuid, "awake"}, NetSafeMoveInvisibleCollision)
  else
    Event.Create(Event.TimerRelative, {1}, ClientMoveCollision)
  end
end

function NetEventCallback(nEventId, tArgs)
  if nEventId == NETEVENT_SETSTARTUPWEAPONS then
    bMoveInvisibleCollision = nil
    Event.Create(Event.ObjectHibernation, {
      Player.GetLocalCharacter(),
      "awake"
    }, SetStartupWeapons)
    if tArgs[1] == 1 then
      ClientMoveCollision()
    end
  elseif nEventId == NETEVENT_MOVECOLLISION then
    ClientMoveCollision()
  end
end

function LoadAssets(self, tSaveData)
  local tLayersToRemove = {}
  local tLayersToAdd = {
    "Vz_State_PmcCon001",
    "VZ_State_Pmc_Pristine"
  }
  if self:_GetFlag("HijackInitiated") then
    table.insert(tLayersToRemove, "vz_state_pmccon001_VillaSoldiers")
  else
    table.insert(tLayersToAdd, "vz_state_pmccon001_VillaSoldiers")
  end
  MrxLayerManager.Remove(tLayersToRemove, function()
    MrxLayerManager.Add(tLayersToAdd, self.AssetsLoaded, {self})
  end)
  SetStartupWeapons()
end

function Activated(self)
  self:ActivateMission()
end

function ActivateMission(self)
  bMoveInvisibleCollision = nil
  MrxTaskContract.Activated(self)
  self:_CreateEvent(Event.ObjectDeath, {
    Pg.GetGuidByName("PMC_CentralBuilding")
  }, function(self)
    self:_SetCancelMessage("[PmcCon001.Terms.Cancel03]")
    self:Cancel()
  end, {self})
  self:_CreateEvent(Event.ObjectDeath, {
    Pg.GetGuidByName("_pmcoutpost_bld_hqsuites 0x000cf8c2")
  }, function(self)
    self:_SetCancelMessage("[PmcCon001.Terms.Cancel03]")
    self:Cancel()
  end, {self})
  self:_CreateEvent(Event.ObjectDeath, {
    Pg.GetGuidByName("pmcoutpost_bld_hqgarage")
  }, function(self)
    self:_SetCancelMessage("[PmcCon001.Terms.Cancel03]")
    self:Cancel()
  end, {self})
  Graphics.Atmosphere.ChangeLineRegionSetting(Pg.GetGuidByName("rgn_atmo_PMCinterior"), "warzone")
  Ai.SetLaneActive(Pg.GetGuidByName("Road 0x000a7417"), 1, false)
  Ai.SetLaneActive(Pg.GetGuidByName("Road 0x000d3c9a"), 1, false)
  if self:_GetFlag("HijackInitiated") then
    Debug.Printf("************** PMC 001: INVESTIGATE VILLA FLAG RETRIEVED")
    self:GoToVillaInterior(self)
    GateCloser(self)
  elseif self:_GetFlag("VillaReached") then
    Debug.Printf("************** PMC 001: VILLA REACHEDCOMPLETE FLAG RETRIEVED")
    self:KillSolanoEntourage01(self)
  else
    IntroBanter(self)
    VZJeepPursuitRegionActivate(self)
    SetupGateCloser(self)
    SetUpBanterRegion(self)
    local iMoveCollision = 0
    if bMoveInvisibleCollision then
      iMoveCollision = 1
    end
    Net.SendCustomEvent("PmcCon001", NETEVENT_SETSTARTUPWEAPONS, {iMoveCollision})
    PmcInvulnerable(self)
    MrxFactionManager.DisableReporting(true)
  end
end

function OnPlayerJoined(self, iPlayerId, uPlayerGuid, uCharGuid)
  local iMoveCollision = 0
  if bMoveInvisibleCollision then
    iMoveCollision = 1
  end
  Net.SendCustomEvent("PmcCon001", NETEVENT_SETSTARTUPWEAPONS, {iMoveCollision})
end

function SetStartupWeapons()
  local uChar = Player.GetLocalCharacter()
  local uRifle = Pg.GetGuidByName("Assault Rifle")
  local uC4 = Pg.GetGuidByName("C4")
  local uGrenade = Pg.GetGuidByName("Grenade")
  Human.Inventory.SetAllWeapons(uChar, {
    uRifle,
    uC4,
    uGrenade
  })
  local uPrimary = Human.Inventory.GetPrimaryWeapon(uChar)
  Weapon.SetReserveAmmo(uPrimary, Weapon.GetMaxReserveAmmo(uPrimary))
end

function IntroBanter(self)
  MrxVoSequence.Start({
    "Fiona-In-Mission-Contract-Pmc01-72",
    {
      mattias = "Mattias-In-Mission-Contract-Pmc01-73",
      jennifer = "Jennifer-In-Mission-Contract-Pmc01-74",
      chris = "Chris-In-Mission-Contract-Pmc01-75"
    },
    "Fiona-In-Mission-Contract-Pmc01-76",
    5,
    {
      SetupGoToObjective,
      {self}
    }
  })
end

function VZJeepPursuitRegionActivate(self)
  self:_CreateEvent(Event.Boundary, {
    Player.GetAnyCharacter(),
    Pg.GetGuidByName("Region_PMC001_VZCheckpointRegion"),
    "enter",
    false
  }, JeepPursuit01, {self})
  Debug.Printf("********************* VZ Jeep Pursuit Region Active! ")
end

function JeepPursuit01(self)
  self:_CreateEvent(Event.TimerRelative, {10})
  local tPursuitTable = {
    {
      "Driving",
      {
        {
          "Car",
          "M151 .50Cal (VZ) (DriverGunner)",
          1
        }
      },
      {
        {"Car", 1}
      }
    },
    {
      "Stopped",
      {
        {
          "Car",
          "M151 .50Cal (VZ) (DriverGunner)",
          1
        }
      },
      {
        {"Car", 1}
      }
    },
    {
      "Offroad",
      {
        {
          "Car",
          "M151 .50Cal (VZ) (DriverGunner)",
          1
        }
      },
      {
        {"Car", 1}
      }
    },
    {
      "Heli",
      {
        {
          "Car",
          "M151 .50Cal (VZ) (DriverGunner)",
          1
        }
      },
      {
        {"Car", 1}
      }
    }
  }
  MrxFactionManager.SetCustomPursuit(Pg.GetGuidByName("VZ"), -1, tPursuitTable)
  self:_CreateEvent(Event.Boundary, {
    Player.GetAnyCharacter(),
    Pg.GetGuidByName("Region_PMC001_EndPursuit"),
    "enter",
    false
  }, StopPursuit, {self})
end

function StopPursuit(self)
  Debug.Printf("SSSSSSSSSSSSSSSStop stop Pursuit ")
  MrxFactionManager.ClearCustomPursuit()
  MrxVoSequence.Start({
    "Fiona.PMC.Aaron01"
  })
end

function SetupGateCloser(self)
  self:_CreateEvent(Event.Boundary, {
    Player.GetAnyCharacter(),
    Pg.GetGuidByName("Region_PmcCon001_VillaWideRegion"),
    "enter",
    false
  }, GateCloser, {self})
  Debug.Printf("********************* Gate Closer Region Active! ")
end

function GateCloser(self)
  local tPmcEntrances = {
    "Pmc001_Door_Front",
    "PMC001_Garage_01",
    "PMC001_Garage_02",
    "PMC001_Garage_03",
    "PMC001_Rear_BottomDoor",
    "PMC001_Rear_TopDoor"
  }
  
  local function CloseDoor(uGuid)
    Object.CloseGate(uGuid)
  end
  
  for i, door in pairs(tPmcEntrances) do
    local uGuid = Pg.GetGuidByName(door)
    Event.Create(Event.ObjectHibernation, {uGuid, "awake"}, CloseDoor, {uGuid})
  end
  Object.CloseGate(Pg.GetGuidByName("pmc_middle_gate"))
end

function SetupGoToObjective(self)
  Debug.Printf("********************* PMC 001: should be running Go To Objective now!!! ")
  local uHeroPMC1 = Player.GetAnyCharacter()
  oGotoVilla = self:CreateChild({
    sName = "PMC001: Go to the Villa",
    sModuleName = "MrxTaskObjectiveDeliver",
    vTgtInclude = uHeroPMC1,
    vDestLoc = "loc_PMC1a",
    vDestRegion = "reg_PMC001a",
    bStop = false,
    bXZOnly = true,
    sDspShortDesc = "[PmcCon001.Objectives.001]",
    vVoSeqOnAdd = {
      "Fiona-In-Mission-Contract-Pmc01-78",
      "Fiona-In-Mission-Contract-Pmc01-79"
    },
    tOnComplete = {
      {
        KillSolanoEntourage01,
        {self}
      }
    },
    tOnCancel = {
      {
        self.Cancel,
        {self}
      }
    }
  })
end

function KillSolanoEntourage01(self)
  self:_SetFlag("VillaReached")
  _Checkpoint({
    "Checkpoint_PMC001_VillaReached"
  })
  Debug.Printf("********************* PMCCON001: FLAG SET")
  local uTarget01 = Pg.GetGuidByName("PMC001_HVT_01")
  local uTarget02 = Pg.GetGuidByName("PMC001_HVT_02")
  local uTarget03 = Pg.GetGuidByName("PMC001_HVT_03")
  local uTarget04 = Pg.GetGuidByName("PMC001_HVT_04")
  local uTarget05 = Pg.GetGuidByName("PMC001_HVT_05")
  self.curObj = self:CreateChild({
    sName = "Kill Solano's Entourage!",
    sModuleName = "MrxTaskObjectiveDestroy",
    vTgtInclude = {
      uTarget01,
      uTarget02,
      uTarget03,
      uTarget04,
      uTarget05
    },
    bDspBlp = true,
    sDspShortDesc = "[PmcCon001.Objectives.002]",
    vVoSeqOnAdd = {
      "Fiona-In-Mission-Contract-Pmc01-94",
      {
        mattias = "Mattias-In-Mission-Contract-Pmc01-97",
        jennifer = "Jennifer-In-Mission-Contract-Pmc01-95",
        chris = "Chris-In-Mission-Contract-Pmc01-96"
      }
    },
    tOnComplete = {
      {
        GoToVillaInteriorLayerLoad,
        {self}
      }
    },
    tOnCancel = {
      {
        self.Cancel,
        {self}
      }
    }
  })
end

function GoToVillaInteriorLayerLoad(self)
  GoToVillaInterior(self)
  local tLayersToAdd = {
    "vz_state_PmcCon001_InvestigateVilla"
  }
  MrxLayerManager.Add(tLayersToAdd, {self})
end

function GateOpener(self)
  Debug.Printf("********************* WE ARE INSIDE GATE OPENER FUNCTION ")
  local tPmcEntrances = {
    "Pmc001_Door_Front",
    "PMC001_Rear_BottomDoor",
    "PMC001_Rear_TopDoor"
  }
  local frontdoor = Pg.GetGuidByName("PMC001_Door_Front")
  
  local function OpenDoor(uGuid)
    Object.OpenGate(uGuid)
  end
  
  for i, door in pairs(tPmcEntrances) do
    local uGuid = Pg.GetGuidByName(door)
    Event.Create(Event.ObjectHibernation, {uGuid, "awake"}, OpenDoor, {uGuid})
  end
  self:_CreateEvent(Event.ObjectHibernation, {
    Pg.GetGuidByName("Pmc001_Door_Front"),
    "hibernated"
  }, CheckFrontDoor, {self})
end

function CheckFrontDoor(self)
  Debug.Printf("********************* Checking Front Door - Dehibernation Event Active ")
  self:_CreateEvent(Event.ObjectHibernation, {
    Pg.GetGuidByName("Pmc001_Door_Front"),
    "awake"
  }, GateOpener, {self})
end

function NetSafeMoveInvisibleCollision()
  Debug.Printf("NetSafeMoveInvisibleCollision bMoveInvisibleCollision = " .. tostring(bMoveInvisibleCollision))
  if not bMoveInvisibleCollision then
    local x, y, z = Object.GetPosition(Pg.GetGuidByName("PMC001_FrontDoor_InvisiblePhysics"))
    Object.SetPosition(Pg.GetGuidByName("PMC001_FrontDoor_InvisiblePhysics"), x, y - 30, z)
    if Net.IsServer() then
      Net.SendCustomEvent("PmcCon001", NETEVENT_MOVECOLLISION, {})
    end
    bMoveInvisibleCollision = true
  end
end

function GoToVillaInterior(self)
  local uHeroPMC1 = Player.GetAnyCharacter()
  Debug.Printf("********************* WE ARE INSIDE GO TO VILLA INTERIOR FUNCTION ")
  oGotoVilla = self:CreateChild({
    sName = "PMC001: Go to Villa Interior",
    sModuleName = "MrxTaskObjectiveDeliver",
    vTgtInclude = uHeroPMC1,
    vDestLoc = "loc_PMC001_InvestigateVilla01",
    vDestRegion = "Region_PMC001_InvestigateVilla01",
    bStop = false,
    bXZOnly = true,
    sDspShortDesc = "[PmcCon001.Objectives.006]",
    vVoSeqOnAdd = {
      {
        "Fiona-In-Mission-Contract-Pmc01-11"
      }
    },
    tOnComplete = {
      {
        GoToVillaInterior02,
        {self}
      }
    },
    tOnCancel = {
      {
        self.Cancel,
        {self}
      }
    }
  })
end

function GoToVillaInterior02(self)
  local uHeroPMC1 = Player.GetAnyCharacter()
  self:_SetFlag("HijackInitiated")
  _Checkpoint({
    "TankHijackCheckpoint"
  })
  Debug.Printf("********************* PMCCON001: FLAG SET")
  Debug.Printf("********************* PMCCON001: INSIDE GO TO VILLA INTERIOR 02 FUNCTION")
  NetSafeMoveInvisibleCollision()
  GateOpener(self)
  oGotoVilla = self:CreateChild({
    sName = "PMC001: Investigate Villa Interior",
    sModuleName = "MrxTaskObjectiveDeliver",
    vTgtInclude = uHeroPMC1,
    vDestLoc = "loc_PMC001_InvestigateVilla02",
    vDestRegion = "Region_PMC001_VillaInterior",
    bStop = false,
    bXZOnly = true,
    sDspShortDesc = "[PmcCon001.Objectives.007]",
    vVoSeqOnAdd = {
      {
        "Fiona-In-Mission-Contract-Pmc01-38"
      }
    },
    tOnComplete = {
      {
        ObjectInSightCheck,
        {self}
      }
    },
    tOnCancel = {
      {
        self.Cancel,
        {self}
      }
    }
  })
end

function ObjectInSightCheck(self)
  self:_CreateEvent(Event.ObjectIsVisible, {
    Pg.GetGuidByName("_pmcoutpost_column 0x000a74ec")
  }, LoadTank, {self})
end

function LoadTank(self)
  local function OnTankLoaded()
    self:_CreateEvent(Event.ObjectHibernation, {
      Pg.GetGuidByName("PMC001_EntourageScorpion"),
      
      "awake"
    }, ActionHijackTank(self))
  end
  
  MrxLayerManager.Add("vz_state_PmcCon001_ActionHijackTutorial", OnTankLoaded, {self})
  MrxVoSequence.Start("Fiona-In-Mission-Contract-Pmc01-102")
end

function GarageSmash(self)
  Object.Kill(Pg.GetGuidByName("PMC001_GarageEntrance"))
  Debug.Printf("********************* PMCCON001:", "Garage Entrance Should Be Nuked!")
  vGoal = Ai.Goal({
    AIGuid = Vehicle.GetDriver(Pg.GetGuidByName("PMC001_EntourageScorpion")),
    Goal = "PathMove",
    Target = Pg.GetGuidByName("Path_PMC001_GarageSmash"),
    Priority = "HiPri",
    Haste = 1
  })
  Sound.CueSound(Pg.GetGuidByName("PMC001_EntourageScorpion"), "exp_bust_thru_wall")
  Debug.Printf("********************* PMCCON001:", vGoal)
  Debug.Printf("********************* PMCCON001:", "Tank Should Be Moving!!")
end

function RideDragonAchievement(self, uTank)
  local uDriver = Vehicle.GetDriver(uTank)
  if uDriver then
    local uPlayer = Object.IsPlayerControlled(uDriver)
    if uPlayer then
      MrxAchievements.NetGrantAchievement("ACHIEVEMENT_RIDE_DRAGON", uPlayer)
    end
  end
end

function ActionHijackTank(self)
  local uPmcTank = Pg.GetGuidByName("PMC001_EntourageScorpion")
  MrxTutorialManager.ShowMessage("[Tutorial.ActionHijack]")
  if Object.IsAlive(Pg.GetGuidByName("PMC001_EntourageScorpion")) then
    GarageSmash(self)
    Vehicle.Usable(Pg.GetGuidByName("PMC001_EntourageScorpion"), false)
    oHijackTank = self:CreateChild({
      sName = "Hijack the Tank",
      sModuleName = "MrxTaskObjectiveEnterVehicle",
      sActionLabel = "Hijack",
      vTgtInclude = Pg.GetGuidByName("PMC001_EntourageScorpion"),
      sDspShortDesc = "[PmcCon001.Objectives.004]",
      vVoSeqOnAdd = {
        {
          mattias = "Mattias-In-Mission-Contract-Pmc01-101",
          jennifer = "Jennifer-In-Mission-Contract-Pmc01-99",
          chris = "Chris-In-Mission-Contract-Pmc01-100"
        },
        "Fiona-In-Mission-Contract-Pmc01-102",
        {
          mattias = "Mattias-In-Mission-Contract-Pmc01-107",
          jennifer = "Jennifer-In-Mission-Contract-Pmc01-108",
          chris = "Chris-In-Mission-Contract-Pmc01-109"
        },
        "Fiona.vo.fio.vp1fio07"
      },
      fOnComplete = function()
        RideDragonAchievement(self, uPmcTank)
        KillSolanoEntourage02Load(self)
        MrxTutorialManager.HideMessage()
      end,
      self:_CreateEvent(Event.ObjectInSeat, {
        Player.GetAnyCharacter(),
        Pg.GetGuidByName("PMC001_EntourageScorpion"),
        "D",
        "E"
      }, function()
        oHijackTank:Complete()
      end)
    })
  else
    KillSolanoEntourage02Load(self)
  end
  oTankDestroyed = self:_CreateEvent(Event.ObjectDeath, {
    Pg.GetGuidByName("PMC001_EntourageScorpion")
  }, function()
    oHijackTank:Complete()
  end)
end

function KillSolanoEntourage02Load(self)
  MrxVoSequence.Start({
    "Fiona-In-Mission-Contract-Pmc01-48",
    1,
    {
      mattias = "Mattias-In-Mission-Contract-Pmc01-51",
      jennifer = "Jennifer-In-Mission-Contract-Pmc01-52",
      chris = "Chris-In-Mission-Contract-Pmc01-53"
    }
  })
  local tLayersToAdd = {
    "vz_State_PMC001_VillaWaveOne"
  }
  MrxLayerManager.Add(tLayersToAdd, KillSolanoEntourage02, {self})
  local tLayersToAdd = {
    "vz_State_PMC001_VillaWaveOne"
  }
end

function KillSolanoEntourage02(self)
  Wave01(self)
  self.curObj = self:CreateChild({
    sName = "Kill Solano's Entourage!",
    sModuleName = "MrxTaskObjectiveDestroy",
    sTgtLabelFilter = "VZ",
    nQuota = 10,
    bDspBlp = true,
    sDspShortDesc = "[PmcCon001.Objectives.005]",
    tOnComplete = {
      {
        self.RunAndFleeInTerror,
        {self}
      }
    },
    tOnCancel = {
      {
        self.Cancel,
        {self}
      }
    }
  })
end

function Pmc01FionaVOComplete(self)
  MrxVoSequence.Start({
    "Fiona-In-Mission-Contract-Pmc01-12",
    1,
    {
      self.Complete,
      {self}
    }
  })
end

function RunAndFleeInTerror(self)
  tBothWaves = {
    utop02,
    utop03,
    utop04,
    ubottom01,
    ubottom02,
    ugarage02,
    uw2_top01,
    uw2_top03,
    uw2_bottom01,
    uw2_bottom03,
    uw2_bottom04
  }
  Debug.Printf("******************PMC 001: FOR loop about to fire")
  for i, uEntourage in pairs(tBothWaves) do
    if uEntourage then
      Debug.Printf("******************PMC 001: Fleeing VZ:" .. tostring(uEntourage))
      Ai.Goal({
        AIGuid = uEntourage,
        Goal = "MoveTo",
        Target = Pg.GetGuidByName("loc_EntourageFleePoint"),
        Haste = 1,
        Priority = "HiPri"
      })
    end
  end
  MrxVoSequence.Start({
    "Fiona-In-Mission-Contract-Pmc01-12",
    1,
    {
      self.Complete,
      {self}
    }
  })
end

function Wave01(self)
  utop02 = Pg.GetGuidByName("top02")
  utop03 = Pg.GetGuidByName("top03")
  utop04 = Pg.GetGuidByName("top04")
  ubottom01 = Pg.GetGuidByName("bottom01")
  ubottom02 = Pg.GetGuidByName("bottom02")
  ugarage02 = Pg.GetGuidByName("garage02")
  if utop02 then
    Ai.Goal({
      AIGuid = utop02,
      Goal = "PathMove",
      Target = Pg.GetGuidByName("Path_top02"),
      Priority = "hiPri",
      Haste = 1
    })
  else
    Debug.Printf("Wave 01 - Member Dead")
  end
  if utop03 then
    Ai.Goal({
      AIGuid = utop03,
      Goal = "PathMove",
      Target = Pg.GetGuidByName("Path_top03"),
      Priority = "hiPri",
      Haste = 1
    })
  else
    Debug.Printf("Wave 01 - Member Dead")
  end
  if utop04 then
    Ai.Goal({
      AIGuid = utop04,
      Goal = "PathMove",
      Target = Pg.GetGuidByName("Path_top04"),
      Priority = "hiPri",
      Haste = 1
    })
  else
    Debug.Printf("Wave 01 - Member Dead")
  end
  if ubottom01 then
    Ai.Goal({
      AIGuid = ubottom01,
      Goal = "PathMove",
      Target = Pg.GetGuidByName("Path_Bottom01"),
      Priority = "hiPri",
      Haste = 1
    })
  else
    Debug.Printf("Wave 01 - Member Dead")
  end
  if ubottom02 then
    Ai.Goal({
      AIGuid = ubottom02,
      Goal = "PathMove",
      Target = Pg.GetGuidByName("Path_PMC001_WaveTwo_BottomLeft"),
      Priority = "hiPri",
      Haste = 1
    })
  else
    Debug.Printf("Wave 01 - Member Dead")
  end
  if ugarage02 then
    Ai.Goal({
      AIGuid = ugarage02,
      Goal = "PathMove",
      Target = Pg.GetGuidByName("Path_PMC001_GarageSmash"),
      Priority = "hiPri",
      Haste = 1
    })
  else
    Debug.Printf("Wave 01 - Member Dead")
  end
  self:_CreateEvent(Event.TimerRelative, {15}, SetUpWave02, {self})
end

function SetUpWave02(self)
  local tLayersToAdd = {
    "vz_state_Pmc001_VillaWaveTwo"
  }
  MrxLayerManager.Add(tLayersToAdd, Wave02, {self})
end

function Wave02(self)
  uw2_top01 = Pg.GetGuidByName("w2_top01")
  uw2_top02 = Pg.GetGuidByName("w2_top02")
  uw2_top03 = Pg.GetGuidByName("w2_top03")
  uw2_top04 = Pg.GetGuidByName("w2_top04")
  uw2_bottom01 = Pg.GetGuidByName("w2_bottom01")
  uw2_bottom02 = Pg.GetGuidByName("w2_bottom02")
  uw2_bottom03 = Pg.GetGuidByName("w2_bottom03")
  uw2_bottom04 = Pg.GetGuidByName("w2_bottom04")
  if uw2_top01 then
    Ai.Goal({
      AIGuid = uw2_top01,
      Goal = "PathMove",
      Target = Pg.GetGuidByName("Path_PMC001_WaveTwo_TopRight"),
      Priority = "hiPri",
      Haste = 1
    })
  else
    Debug.Printf("Wave 02 - Member Dead")
  end
  if uw2_top03 then
    Ai.Goal({
      AIGuid = uw2_top03,
      Goal = "PathMove",
      Target = Pg.GetGuidByName("Path_PMC001_WaveTwo_TopLeft"),
      Priority = "hiPri",
      Haste = 1
    })
  else
    Debug.Printf("Wave 02 - Member Dead")
  end
  if uw2_bottom01 then
    Ai.Goal({
      AIGuid = uw2_bottom01,
      Goal = "PathMove",
      Target = Pg.GetGuidByName("Path_PMC001_WaveTwo_BottomLeft"),
      Priority = "hiPri",
      Haste = 1
    })
  else
    Debug.Printf("Wave 02 - Member Dead")
  end
  if uw2_bottom03 then
    Ai.Goal({
      AIGuid = uw2_bottom03,
      Goal = "PathMove",
      Target = Pg.GetGuidByName("Path_Bottom01"),
      Priority = "hiPri",
      Haste = 1
    })
  else
    Debug.Printf("Wave 02 - Member Dead")
  end
  if uw2_bottom04 then
    Ai.Goal({
      AIGuid = uw2_bottom04,
      Goal = "PathMove",
      Target = Pg.GetGuidByName("Path_Bottom01"),
      Priority = "hiPri",
      Haste = 1
    })
  else
    Debug.Printf("Wave 02 - Member Dead")
  end
end

function SetupCourtyardPanic(self)
  self:_CreateEvent(Event.Boundary, {
    Player.GetAnyCharacter(),
    Pg.GetGuidByName("Region_PMC_FrontGate"),
    "enter",
    false
  }, function()
    self:_CreateEvent(Event.TimerRelative, {2}, CourtyardPanic, {self})
  end)
  Debug.Printf("********************* Courtyard Panic Region Set Up!")
end

function SetUpBanterRegion(self)
  self:_CreateEvent(Event.Boundary, {
    Player.GetAnyCharacter(),
    Pg.GetGuidByName("Region_PMC001_Banter01"),
    "enter",
    false
  }, Banter, {self})
  Debug.Printf("********************* Banter Region Set Up!")
end

function Banter(self)
  MrxVoSequence.Start({
    "Fiona-In-Mission-Contract-Pmc01-83",
    {
      mattias = "Mattias-In-Mission-Contract-Pmc01-86",
      jennifer = "Jennifer-In-Mission-Contract-Pmc01-84",
      chris = "Chris-In-Mission-Contract-Pmc01-85"
    }
  })
end

function PmcInvulnerable(self)
end

function Cleanup(self)
  local tPlayers = Player.GetAllPlayers()
  for i, uPlayerGuid in ipairs(tPlayers) do
    local uCharacter = Player.GetCharacter(uPlayerGuid)
    Human.ForceExitSeatNoSnap(uCharacter)
  end
  for i, uDoor in pairs(tPmcDoors) do
    local uDoor = Pg.GetGuidByName(uDoor)
    Hud.Radar:RemoveObjective({uDoor})
  end
  local tLayersMarkForRemove = {
    "Vz_State_PmcCon001",
    "VZ_State_Pmc_Pristine",
    "vz_state_PmcCon001_ActionHijackTutorial",
    "vz_state_PmcCon001_InvestigateVilla",
    "vz_state_pmccon001_VillaSoldiers",
    "vz_state_Pmc001_VillaWaveOne",
    "vz_state_Pmc001_VillaWaveTwo"
  }
  for _, sLayer in pairs(tLayersMarkForRemove) do
    MrxLayerManager.MarkForRemoval(sLayer)
  end
  Ai.SetRelation(Pg.GetGuidByName("VZ"), Pg.GetGuidByName("PMC"), -100)
  MrxFactionManager.DisableReporting(false)
  MrxFactionManager.ClearPursuitLock()
  Graphics.Atmosphere.ChangeLineRegionSetting(Pg.GetGuidByName("rgn_atmo_PMCinterior"), "default")
  MrxTaskContract.Cleanup(self)
end
