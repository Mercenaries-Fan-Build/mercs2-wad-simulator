inherit("MrxTaskContract")
import("MrxGuiManager")
import("MrxSubtitle")
import("MrxVoSequence")
import("MrxClusterBomb")
import("MrxSupportData")
import("MrxFactionManager")
import("MrxState")
import("MrxSupport")
import("MrxTutorialManager")
import("MrxAchievements")
import("MrxGuiSatellite")
import("MrxCopterDrop")
import("MrxSupportData")
NETEVENT_CLIENTSETUP = 0
iInsideMinigame = 0
iGate = 1

function NetEventCallback(nEventId, tArgs)
  if nEventId == NETEVENT_CLIENTSETUP then
    Event.Create(Event.ObjectHibernation, {
      Player.GetLocalCharacter(),
      "awake"
    }, SetStartupWeapons)
    Graphics.Atmosphere.ChangeLineRegionSetting(Pg.GetGuidByName("rgn_atmo_carmonaislandrain"), "night")
  end
end

function LoadAssets(self, tSaveData)
  local tLayersToAdd, tLayersToRemove, fCallback, tCallbackArgs
  if self:_GetFlag("VZ001CP02") then
    tLayersToAdd = {
      "Vz_State_VzaCon001",
      "vz_state_VzaCon001_Pristine"
    }
    tLayersToRemove = {
      "Vz_State_VzaCon001_CP01",
      "Vz_State_VzaCon001_CP02",
      "Vz_State_VzaCon001_PreGate1",
      "Vz_State_VzaCon001_PreGate2"
    }
    fCallback = self.AssetsLoaded
    tCallbackArgs = {self}
  elseif self:_GetFlag("VZ001CP01") then
    tLayersToAdd = {
      "Vz_State_VzaCon001",
      "vz_state_VzaCon001_Pristine",
      "Vz_State_VzaCon001_CP02",
      "Vz_State_VzaCon001_PreGate2"
    }
    tLayersToRemove = {
      "Vz_State_VzaCon001_CP01",
      "Vz_State_VzaCon001_PreGate1"
    }
    fCallback = self.AssetsLoaded
    tCallbackArgs = {self}
  else
    tLayersToAdd = {
      "Vz_State_VzaCon001",
      "vz_state_VzaCon001_Pristine",
      "Vz_State_VzaCon001_CP01",
      "Vz_State_VzaCon001_CP02",
      "Vz_State_VzaCon001_PreGate1",
      "Vz_State_VzaCon001_PreGate2"
    }
    tLayersToRemove = {}
    fCallback = self.StandardSetup
    tCallbackArgs = {self}
    SetStartupWeapons()
  end
  MrxLayerManager.Remove(tLayersToRemove, MrxLayerManager.Add, {
    tLayersToAdd,
    fCallback,
    tCallbackArgs
  })
end

function StandardSetup(self)
  if Net.DoneReloadingLayers then
    Net.DoneReloadingLayers()
  end
  local uBoat = Pg.GetGuidByName("VzaCon001_StartingBoat")
  
  local function _PutPlayersInBoat(uBoat)
    _tPlayers = {}
    local tPlayers = Player.GetAllPlayers()
    for i, uPlayer in ipairs(tPlayers) do
      local uChar = Player.GetCharacter(uPlayer)
      local uWeapon = Human.Inventory.GetPrimaryWeapon(uChar)
      if uWeapon then
        Weapon.SetReserveAmmo(uWeapon, Weapon.GetMaxReserveAmmo(uWeapon))
      end
      Human.SetState(uChar, "Upright", "Idle")
      local sSeat
      if i == 1 then
        Vehicle.Enter(uBoat, uChar, "d", false)
        sSeat = "D"
      else
        Net.SendEvent_JoinPOForceRequest()
        Vehicle.Enter(uBoat, uChar, "p", false)
        sSeat = "P"
      end
      self:_CreateEvent(Event.ObjectInSeat, {
        uChar,
        uBoat,
        sSeat,
        "E"
      }, self.EnsureHeroesInBoat, {self})
      _tPlayers[uChar] = false
    end
    if Net.IsServer() then
      Net.SendEvent_ForceClientTether()
    end
    if not Net.IsMultiplayer() or Net.IsServer() then
      MrxStatsManager.DeleteVehicleTimer()
      MrxStatsManager.AddVehicleTimer()
    end
  end
  
  Event.Create(Event.ObjectHibernation, {uBoat, "a"}, _PutPlayersInBoat, {uBoat})
end

function EnsureHeroesInBoat(self, uOccupant)
  _tPlayers[uOccupant] = true
  local bAllIn = true
  for uChar, bEntered in pairs(_tPlayers) do
    if not bEntered then
      bAllIn = false
      break
    end
  end
  if bAllIn then
    local tPlayers = Player.GetAllPlayers()
    if tPlayers then
      for _, uPlayer in ipairs(tPlayers) do
        local uCam = Player.GetCamera(uPlayer)
        if uCam and Camera.StopBlending then
          Camera.StopBlending(uCam)
        end
      end
    end
    self:AssetsLoaded()
    _tPlayers = nil
  else
    Debug.Printf("waiting for all players to get in boat")
  end
end

function Activated(self)
  bCinematicSkipped = false
  MrxTaskContract.Activated(self)
  MrxFactionManager.DisableReporting(true)
  self:_CreateEvent(Event.ObjectInSeat, {
    Player.GetAnyCharacter(),
    "VzaCon001_StartingBoat",
    "d",
    "x"
  }, MrxTutorialManager.HideMessage, {false, "VzaCon001"})
  self:_CreateEvent(Event.ObjectDeath, {
    Pg.GetGuidByName("VzaCon001_StartingBoat")
  }, MrxTutorialManager.HideMessage, {false, "VzaCon001"})
  self:_CreateEvent(Event.Boundary, {
    Player.GetAnyCharacter(),
    Pg.GetGuidByName("region_killvzaheli"),
    "enter",
    false
  }, StopTheMusic, {self})
  Event.Create(Event.ObjectHibernation, {
    Pg.GetGuidByName("Carmona_VzaCon001"),
    "awake"
  }, function()
    DropCarmonaWeapons(self)
  end)
  self:_CreateEvent(Event.Boundary, {
    Player.GetAnyCharacter(),
    Pg.GetGuidByName("region_brokenbridge"),
    "enter",
    false
  }, BrokenBridgeEncounter, {self})
  self:_CreateEvent(Event.Boundary, {
    Player.GetAnyCharacter(),
    Pg.GetGuidByName("region_vza_handbrake"),
    "enter",
    false
  }, HandbrakeTutorial, {self})
  self:_CreateEvent(Event.Boundary, {
    Player.GetAnyCharacter(),
    Pg.GetGuidByName("region_vza_usegrenade"),
    "enter",
    false
  }, GrenadeHint, {self})
  self:_CreateEvent(Event.Boundary, {
    Player.GetAnyCharacter(),
    Pg.GetGuidByName("region_vza_tankswitch"),
    "enter",
    false
  }, TankSwitch, {self})
  self:_CreateEvent(Event.ScriptEvent, {
    "Satellite Targetting Success",
    function()
      return true
    end
  }, AirstrikeMinigame_SuccessVO, {self})
  self:_CreateEvent(Event.ScriptEvent, {
    "Satellite Targetting Start",
    function()
      return true
    end
  }, AirstrikeMinigame_InsideVO, {self})
  if self:_GetFlag("VZ001CP02") then
    self:DeliverTank()
  elseif self:_GetFlag("VZ001CP01") then
    self:GetToFirstWaypoint()
  else
    self:BoatApproach()
    SetupAirstrikeEvent(self)
  end
  Graphics.Atmosphere.ChangeLineRegionSetting(Pg.GetGuidByName("rgn_atmo_carmonaislandrain"), "night")
  Net.SendCustomEvent("VzaCon001", NETEVENT_CLIENTSETUP, {}, true)
end

function OnPlayerJoined(self, iPlayerId, uPlayerGuid, uCharGuid)
  Net.SendCustomEvent("VzaCon001", NETEVENT_CLIENTSETUP, {}, true)
end

function SetStartupWeapons()
  local uPlayer = Player.GetLocalPlayer()
  local uChar = Player.GetLocalCharacter()
  local uRifle = Pg.GetGuidByName("Carbine")
  local uC4 = Pg.GetGuidByName("C4")
  local uGrenade = Pg.GetGuidByName("Grenade")
  Human.Inventory.SetAllWeapons(uChar, {
    uRifle,
    uGrenade,
    uC4
  })
  local uPrimary = Human.Inventory.GetPrimaryWeapon(uChar)
  Event.Create(Event.ObjectHibernation, {uPrimary, "awake"}, FillWeapon, {uPrimary})
end

function FillWeapon(uGuid)
  Weapon.SetReserveAmmo(uGuid, Weapon.GetMaxReserveAmmo(uGuid))
end

function BoatApproach(self)
  self:_CreateEvent(Event.TimerRelative, {0.1}, self.GetToBeach, {self})
end

function GetToBeach(self)
  Sound.SetDynamicMusic(false)
  MrxMusic.PlaySpecialMusic("mu_nomission_water_threat_01")
  oGetToBeach = self:CreateChild({
    sName = "VZA001: Go to the Beach",
    sModuleName = "MrxTaskObjectiveDeliver",
    vTgtInclude = Player.GetAnyCharacter(),
    vDestLoc = "loc_vza_beach",
    vDestRegion = "region_vza_beach",
    bStop = false,
    bXZOnly = true,
    sDspShortDesc = "[VzaCon001.Objectives.001]",
    vVoSeqOnAdd = {
      "Fiona-In-Mission-Contract-Vz01-146",
      0.5,
      {
        mattias = "Mattias-In-Mission-Contract-Vz01-147",
        jennifer = "Jennifer-In-Mission-Contract-Vz01-148",
        chris = "Chris-In-Mission-Contract-Vz01-149"
      },
      0.5,
      "Fiona-In-Mission-Contract-Vz01-150",
      0.5
    },
    tOnComplete = {
      {
        self.BoatCheck,
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

function BoatCheck(self)
  if Object.IsPlayerControlled(Pg.GetGuidByName("VzaCon001_StartingBoat")) then
    MrxTutorialManager.BeginCustomTutorial("VzaCon001")
    MrxTutorialManager.ShowMessage("[Tutorial.EnterExit]", false, "VzaCon001")
    self:_CreateEvent(Event.ObjectInSeat, {
      Player.GetAnyCharacter(),
      Pg.GetGuidByName("VzaCon001_StartingBoat"),
      "D",
      "X"
    }, self.DropInWeapon, {self})
  else
    self.DropInWeapon(self)
  end
end

function DropInWeapon(self)
  MrxTutorialManager.HideMessage(false, "VzaCon001")
  MrxVoSequence.Start({
    "Fiona-In-Mission-Contract-Vz01-221",
    0.5,
    {
      mattias = "Mattias-In-Mission-Contract-Vz01-232",
      jennifer = "Jennifer-In-Mission-Contract-Vz01-177",
      chris = "Chris-In-Mission-Contract-Vz01-178"
    }
  })
  local uPoint1 = Pg.GetGuidByName("loc_vza_weapondrop")
  local x, y, z = Object.GetPosition(uPoint1)
  local uPoint2 = Pg.GetGuidByName("loc_vza_helispawn1")
  local a, b, c = Object.GetPosition(uPoint2)
  local uHeli, uCargo = MrxCopterDrop.Create("VZF", "Supply Drop (GL)", x, y, z, true, a, b, c)
  self:_CreateEvent(Event.TimerRelative, {3}, WaitForWeapon, {self, uCargo})
end

function WaitForWeapon(self, uCargo)
  oWaitForWeapon = self:CreateChild({
    sName = "VZA001: Wait for Weapon",
    sModuleName = "MrxTaskObjectiveDeliver",
    vTgtInclude = {uCargo},
    vDestLoc = "loc_vza_weapondrop",
    vDestRegion = "region_vza_weapondrop",
    bStop = false,
    bXZOnly = false,
    sDspShortDesc = "[VzaCon001.Objectives.002]",
    tOnComplete = {
      {
        self.BeachGuardsCheck,
        {self}
      }
    },
    tOnCancel = {
      {
        self.FailSafe,
        {self}
      }
    }
  })
  self:MeleeBashTuteSetup(uCargo)
end

function FailSafe(self)
  MrxVoSequence.Start({
    "Fiona-In-Mission-Contract-Vz01-240"
  })
  oWaitForWeapon:Complete()
end

function MeleeBashTuteSetup(self, uCargo)
  local uLocation = Pg.GetGuidByName("loc_vza_weapondrop")
  self:_CreateEvent(Event.ObjectProximity, {
    Player.GetPrimaryCharacter(),
    uLocation,
    "<",
    10,
    false,
    false
  }, MeleeBashTute, {self})
  self:_CreateEvent(Event.ObjectDeath, {uCargo}, RemoveBashTute, {self})
end

function RemoveBashTute(self)
  MrxTutorialManager.HideMessage(false, "VzaCon001")
  oWaitForWeapon:Complete()
end

function MeleeBashTute(self)
  MrxTutorialManager.ShowMessage("[Tutorial.Melee]", false, "VzaCon001")
  self:_CreateEvent(Event.TimerRelative, {5}, MrxTutorialManager.HideMessage, {false, "VzaCon001"})
end

function BeachGuardsCheck(self)
  tSquad = {
    Pg.GetGuidByName("vza_beachguard_1"),
    Pg.GetGuidByName("vza_beachguard_2"),
    Pg.GetGuidByName("vza_beachguard_3"),
    Pg.GetGuidByName("vza_beachguard_4")
  }
  local bAnySquadMemberAlive = false
  for i, uSoldier in pairs(tSquad) do
    if Object.IsAlive(uSoldier) then
      bAnySquadMemberAlive = true
      break
    end
  end
  if bAnySquadMemberAlive then
    KillBeachGuards(self)
  else
    DestroyGateSetup(self)
  end
end

function KillBeachGuards(self)
  oBeachEncounter = self:CreateChild({
    sName = "VZA001: EliminateBeachGuardsVZ",
    sModuleName = "MrxTaskObjectiveDestroy",
    vTgtInclude = {
      "vza_beachguard_1",
      "vza_beachguard_2",
      "vza_beachguard_3",
      "vza_beachguard_4"
    },
    sDspShortDesc = "[VzaCon001.Objectives.003]",
    vVoSeqOnAdd = {
      "Fiona-In-Mission-Contract-Vz01-152",
      0.5,
      {
        mattias = "Mattias-In-Mission-Contract-Vz01-153",
        jennifer = "Jennifer-In-Mission-Contract-Vz01-154",
        chris = "Chris-In-Mission-Contract-Vz01-155"
      }
    },
    tOnComplete = {
      {
        self.DestroyGateSetup,
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
  SetupShowShootTutorial(self)
end

function SetupShowShootTutorial(self)
  self:_CreateEvent(Event.ObjectProximity, {
    Player.GetAnyCharacter(),
    Pg.GetGuidByName("vza001_gate"),
    "<",
    125,
    false,
    false
  }, ShowShootTutorial, {self})
end

function ShowShootTutorial(self)
  MrxTutorialManager.ShowMessage([[
[Tutorial.Shoot]
[Tutorial.Reload] ]], false, "VzaCon001")
  self:_CreateEvent(Event.TimerRelative, {10}, MrxTutorialManager.HideMessage, {false, "VzaCon001"})
  self:_CreateEvent(Event.TimerRelative, {12}, MrxTutorialManager.ShowMessage, {
    [=[
[Tutorial.Zoom]
[Tutorial.SwitchWeapons]]=],
    false,
    "VzaCon001"
  })
end

function GatedPDAObjectiveSetup(self)
  MrxTutorialManager.HideMessage(false, "VzaCon001")
  self:_CreateEvent(Event.TimerRelative, {2}, MrxTutorialManager.ShowMessage, {
    "[Tutorial.OpenPDA]",
    false,
    "VzaCon001"
  })
  self:_CreateEvent(Event.ScriptEvent, {
    "PDA Open",
    function(tData)
      Debug.Printf(tostring(tData))
      return Player.GetLocalPlayer() == tData.uPlayer
    end
  }, GatedPDAObjective, {self})
end

function GatedPDAObjective(self)
  self:_CreateEvent(Event.TimerRelative, {0.5, true}, MrxVoSequence.Start, {
    "Fiona-In-Mission-Contract-Vz01-242"
  })
  self:_CreateEvent(Event.ScriptEvent, {
    "PDA Close",
    function(tData)
      return uPlayer == tData[1]
    end
  }, DestroyGateSetupTwo, {self})
end

function DestroyGateSetup(self)
  DestroyGate(self)
  RemoveGrenadeHint(self)
  self:_CreateEvent(Event.TimerRelative, {1}, DestroyGateSetupVO, {self})
end

function DestroyGateSetupVO(self)
  MrxVoSequence.Start({
    "Fiona-In-Mission-Contract-Vz01-208",
    0.5,
    "Fiona-In-Mission-Contract-Vz01-240"
  })
  self:_CreateEvent(Event.TimerRelative, {10}, GatedPDAObjectiveSetup, {self})
end

function DestroyGateSetupTwo(self)
  RemoveGrenadeHint(self)
  MrxVoSequence.Start({
    "Fiona-In-Mission-Contract-Vz01-209",
    0.5,
    "VZSoldier-In-Mission-Contract-Vz01-210",
    0.5,
    "Fiona-In-Mission-Contract-Vz01-211"
  })
  self:_CreateEvent(Event.TimerRelative, {13}, ShowGateTutorial, {self})
  self:_CreateEvent(Event.TimerRelative, {13}, AddFreebies, {"self"})
  self:_CreateEvent(Event.TimerRelative, {10}, ConfirmationFlyby, {self})
end

function DestroyGate(self)
  MrxTutorialManager.HideMessage(false, "VzaCon001")
  if Object.IsAlive(Pg.GetGuidByName("vza001_gate")) then
    oDestroyGate = self:CreateChild({
      sName = "VZA001: DestroyGate",
      sModuleName = "MrxTaskObjectiveDestroy",
      vTgtInclude = {
        "vza001_gate"
      },
      sDspShortDesc = "[VzaCon001.Objectives.005]",
      tOnComplete = {
        {
          MrxTutorialManager.HideMessage,
          {false, "VzaCon001"}
        },
        {
          PostGate,
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
  else
    PostGate(self)
  end
end

function ShowGateTutorial(self)
  MrxTutorialManager.ShowMessage([=[
[Tutorial.SupportMenu]
[SHELL.PCShell.Tutorial_UseSupport_PC]]=], false, "VzaCon001")
end

function PostGate(self)
  MrxAchievements.NetGrantAchievement("ACHIEVEMENT_SCHOOLS_OUT")
  Debug.Printf("*********************************************************************** VZACON001: Achievement")
  FirstCheckpoint(self)
  MrxMusic.StopSpecialMusic()
  self:_CreateEvent(Event.TimerRelative, {3}, function()
    MrxMusic.PlaySpecialMusic("Mu_nomission_jungle_threat_01")
  end)
  self:_CreateEvent(Event.TimerRelative, {3}, CarmonaInHillsVO, {self})
  self:_CreateEvent(Event.TimerRelative, {8}, GetToFirstWaypoint, {self})
end

function CarmonaInHillsVO(self)
  MrxVoSequence.Start({
    "Fiona-In-Mission-Contract-Vz01-32"
  })
end

function SprintTutorial(self)
  MrxTutorialManager.ShowMessage("[Tutorial.Sprint]", false, "VzaCon001")
  self:_CreateEvent(Event.TimerRelative, {6}, MrxTutorialManager.HideMessage, {false, "VzaCon001"})
end

function FirstGateDestroyedVO(self)
  MrxVoSequence.Start({
    {
      mattias = "Mattias-In-Mission-Contract-Vz01-167",
      jennifer = "Jennifer-In-Mission-Contract-Vz01-168",
      chris = "Chris-In-Mission-Contract-Vz01-169"
    },
    "Fiona-In-Mission-Contract-Vz01-50",
    "Fiona-In-Mission-Job-Chi10-08"
  })
end

function GetToFirstWaypoint(self)
  MrxSupportData.RemoveFreebie("VzaCon01_SatBomb")
  SprintTutorial(self)
  SetupAirstrikeEvent(self)
  oGetToFirstWaypoint = self:CreateChild({
    sName = "VZA001: GetToFirstWaypoint",
    sModuleName = "MrxTaskObjectiveDeliver",
    vTgtInclude = Player.GetAnyCharacter(),
    vDestLoc = "loc_vza_waypoint1",
    vDestRegion = "region_vza_waypoint1",
    bStop = false,
    bXZOnly = true,
    sDspShortDesc = "[VzaCon001.Objectives.006]",
    vVoSeqOnAdd = {
      "Fiona-In-Mission-Contract-Vz01-227"
    },
    tOnComplete = {
      {
        self.KillVillageGuards_Failsafe,
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
  self:_CreateEvent(Event.TimerRelative, {5}, MrxTutorialManager.HideMessage, {false, "VzaCon001"})
end

function KillVillageGuards_Failsafe(self)
  if Object.IsAlive(Pg.GetGuidByName("vza_villageguard_1")) or Object.IsAlive(Pg.GetGuidByName("vza_villageguard_2")) or Object.IsAlive(Pg.GetGuidByName("vza_villageguard_3")) then
    KillVillageGuards(self)
  else
    MrxVoSequence.Start({
      "Fiona-In-Mission-Contract-Vz01-222",
      0.5,
      {
        mattias = "Mattias-In-Mission-Contract-Vz01-181",
        jennifer = "Jennifer-In-Mission-Contract-Vz01-182",
        chris = "Chris-In-Mission-Contract-Vz01-183"
      }
    })
    DeliverCar(self)
  end
end

function KillVillageGuards(self)
  oKillVillageGuards = self:CreateChild({
    sName = "VZA001: EliminateVillageGuards",
    sModuleName = "MrxTaskObjectiveDestroy",
    vTgtInclude = {
      "vza_villageguard_1",
      "vza_villageguard_2",
      "vza_villageguard_3"
    },
    sDspShortDesc = "[VzaCon001.Objectives.003]",
    vVoSeqOnAdd = {
      "Fiona-In-Mission-Contract-Vz01-234"
    },
    tOnComplete = {
      {
        self.SetupDeliverCar,
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

function SetupDeliverCar(self)
  MrxVoSequence.Start({
    "Fiona-In-Mission-Contract-Vz01-235"
  })
  DeliverCar(self)
end

function DeliverCar(self)
  local uPoint1 = Pg.GetGuidByName("loc_vza_cardrop")
  local x, y, z = Object.GetPosition(uPoint1)
  local uPoint2 = Pg.GetGuidByName("loc_vza_helispawn2")
  local a, b, c = Object.GetPosition(uPoint2)
  local uHeli, uCargo = MrxCopterDrop.Create("VZF", "M151 Softtop (VZ)", x, y, z, false, a, b, c)
  WaitForCar(self, uCargo)
end

function WaitForCar(self, uCargo)
  oWaitForCar = self:CreateChild({
    sName = "VZA001: Wait for Car",
    sModuleName = "MrxTaskObjectiveDeliver",
    vTgtInclude = {uCargo},
    vDestLoc = "loc_vza_cardrop",
    vDestRegion = "region_vza_getincar",
    bStop = false,
    bXZOnly = false,
    sDspShortDesc = "[VzaCon001.Objectives.007]",
    tOnComplete = {
      {
        self.GetToSecondWaypoint,
        {self}
      }
    },
    tOnCancel = {
      {
        self.GetToSecondWaypoint,
        {self}
      }
    }
  })
end

function GetToSecondWaypoint(self)
  oGetToSecondWaypoint = self:CreateChild({
    sName = "VZA001: GetToSecondWaypoint",
    sModuleName = "MrxTaskObjectiveDeliver",
    vTgtInclude = Player.GetAnyCharacter(),
    vDestLoc = "loc_vza_waypoint2",
    vDestRegion = "region_vza_waypoint2",
    bStop = false,
    bXZOnly = true,
    sDspShortDesc = "[VzaCon001.Objectives.006]",
    vVoSeqOnAdd = {
      "Fiona-In-Mission-Contract-Vz01-236"
    },
    tOnComplete = {
      {
        self.DestroySecondGate,
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

function DestroySecondGate(self)
  iGate = 2
  if Object.IsAlive(Pg.GetGuidByName("vza001_gate2")) then
    oDestroySecondGate = self:CreateChild({
      sName = "VZA001: DestroySecondGate",
      sModuleName = "MrxTaskObjectiveDestroy",
      vTgtInclude = {
        "vza001_gate2"
      },
      sDspShortDesc = "[VzaCon001.Objectives.005]",
      vVoSeqOnAdd = {
        "Fiona-In-Mission-Contract-Vz01-225",
        {
          mattias = "Mattias-In-Mission-Contract-Vz01-157",
          jennifer = "Jennifer-In-Mission-Contract-Vz01-158",
          chris = "Chris-In-Mission-Contract-Vz01-159"
        }
      },
      tOnComplete = {
        {
          DeliverTank,
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
    self:_CreateEvent(Event.TimerRelative, {5}, ShowGateTutorial, {self})
    self:_CreateEvent(Event.TimerRelative, {5}, AddFreebies, {"self"})
  else
    DeliverTank(self)
  end
end

function DeliverTank(self)
  MrxSupportData.RemoveFreebie("VzaCon01_SatBomb")
  local uPoint1 = Pg.GetGuidByName("loc_vza_amxdrop")
  local x, y, z = Object.GetPosition(uPoint1)
  local uPoint2 = Pg.GetGuidByName("loc_vza_helispawn3")
  local a, b, c = Object.GetPosition(uPoint2)
  local uHeli, uCargo = MrxCopterDrop.Create("VZHF", "AMX30", x, y, z, false, a, b, c)
  MrxVoSequence.Start({
    "Fiona-In-Mission-Job-All10-06"
  })
  self:_CreateEvent(Event.TimerRelative, {2}, WaitForTank, {self, uCargo})
end

function WaitForTank(self, uCargo)
  oWaitForTank = self:CreateChild({
    sName = "VZA001: Wait for Tank",
    sModuleName = "MrxTaskObjectiveDeliver",
    vTgtInclude = {uCargo},
    vDestLoc = "loc_vza_amxdrop",
    vDestRegion = "region_vza_amxdrop",
    bStop = false,
    bXZOnly = false,
    sDspShortDesc = "[VzaCon001.Objectives.008]",
    vVoSeqOnAdd = {
      "Fiona-In-Mission-Contract-Vz01-218",
      0.5,
      {
        mattias = "Mattias-In-Mission-Contract-Vz01-232",
        jennifer = "Jennifer-In-Mission-Contract-Vz01-230",
        chris = "Chris-In-Mission-Contract-Vz01-229"
      }
    },
    tOnComplete = {
      {
        self.PauseBeforeThirdWaypoint,
        {self}
      }
    },
    tOnCancel = {
      {
        self.PauseBeforeThirdWaypoint,
        {self}
      }
    }
  })
end

function PauseBeforeThirdWaypoint(self)
  self:_CreateEvent(Event.TimerRelative, {2}, GetToThirdWaypoint, {self})
end

function GetToThirdWaypoint(self)
  SecondCheckpoint(self)
  oGetToThirdWaypoint = self:CreateChild({
    sName = "VZA001: GetToThirdWaypoint",
    sModuleName = "MrxTaskObjectiveDeliver",
    vTgtInclude = Player.GetAnyCharacter(),
    vDestLoc = "loc_vza_waypoint3",
    vDestRegion = "region_vza_waypoint3",
    bStop = false,
    bXZOnly = true,
    sDspShortDesc = "[VzaCon001.Objectives.006]",
    vVoSeqOnAdd = {
      "Fiona-In-Mission-Contract-Vz01-227"
    },
    tOnComplete = {
      {
        self.GetToFourthWaypoint,
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

function GetToFourthWaypoint(self)
  oGetToFourthWaypoint = self:CreateChild({
    sName = "VZA001: GetToFourthWaypoint",
    sModuleName = "MrxTaskObjectiveDeliver",
    vTgtInclude = Player.GetAnyCharacter(),
    vDestLoc = "loc_vza_waypoint4",
    vDestRegion = "region_brokenbridge",
    bStop = false,
    bXZOnly = true,
    sDspShortDesc = "[VzaCon001.Objectives.006]",
    vVoSeqOnAdd = {
      "Fiona-In-Mission-Contract-Vz01-227"
    },
    tOnComplete = {
      {
        self.RescueCarmona,
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

function RescueCarmona(self)
  AddFreebies("self")
  oRescueCarmona = self:CreateChild({
    sName = "VZA001: Rescue Carmona",
    sModuleName = "MrxTaskObjectiveDeliver",
    vTgtInclude = Player.GetAnyCharacter(),
    vDestLoc = "loc_vza_hotel",
    vDestRegion = "region_vza_youwin",
    bStop = false,
    bXZOnly = true,
    sDspShortDesc = "[VzaCon001.Objectives.009]",
    vVoSeqOnAdd = {
      "Fiona-In-Mission-Contract-Vz01-244"
    },
    tOnComplete = {
      {
        self.Complete,
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

function GrenadeHint(self)
  MrxTutorialManager.ShowMessage("[Tutorial.Explosive]", false, "VzaCon001")
  self:_CreateEvent(Event.TimerRelative, {10}, MrxTutorialManager.EndCustomTutorial, {"VzaCon001"})
end

function RemoveGrenadeHint(self)
  MrxTutorialManager.HideMessage(false, "VzaCon001")
end

function BrokenBridgeEncounter(self)
  Ai.Goal({
    AIGuid = Pg.GetGuidByName("brokenbridge_mook"),
    Goal = "PathMove",
    Target = Pg.GetGuidByName("path_brokenbridge"),
    Haste = 1,
    Mode = "OneWay",
    Priority = "hiPri"
  })
end

function BrokenBridgeEncounterComplete(self)
end

function DropCarmonaWeapons(self)
  local uChar = Pg.GetGuidByName("Carmona_VzaCon001")
  local tWeapons = Human.Inventory.GetAllWeapons(uChar)
  for i, weapon in pairs(tWeapons) do
    Human.Inventory.DropWeapon(uChar, weapon)
    Object.Remove(weapon)
  end
end

function SetupAirstrikeEvent(self)
  self:_CreateEvent(Event.ScriptEvent, {
    "Satellite Targetting Start",
    function()
      return true
    end
  }, AirstrikeMinigame, {self})
end

function AirstrikeMinigame(self)
  Debug.Printf("*********************************************************************** VZACON001: START AirStrikeMiniGame")
  iInsideMinigame = 1
  MrxTutorialManager.ShowMessage([=[
[Tutorial.MoveCamera]
[SHELL.PCShell.Tutorial_ConfirmTarget_PC]]=])
  Debug.Printf("*********************************************************************** VZACON001: iInsideMinigame==" .. tostring(iInsideMinigame))
  self:_CreateEvent(Event.ScriptEvent, {
    "Satellite Minigame Start",
    function()
      return true
    end
  }, AirstrikeMinigame_Start, {self})
  self:_CreateEvent(Event.ScriptEvent, {
    "Satellite Targetting Success",
    function()
      return true
    end
  }, AirstrikeMinigame_Success, {self})
  self:_CreateEvent(Event.ScriptEvent, {
    "Satellite Targetting Cancelled",
    function()
      return true
    end
  }, AirstrikeMinigame_Cancel, {self})
  Debug.Printf("*********************************************************************** VZACON001: END AirStrikeMiniGame")
end

function AirstrikeMinigame_Start(self)
  MrxTutorialManager.ShowMessage("[SHELL.PCShell.Tutorial_SatMinigame_PC]")
end

function AirstrikeMinigame_Success(self)
  Debug.Printf("*********************************************************************** VZACON001: Entered AirstrikeMinigame_Success")
  self:_CreateEvent(Event.TimerRelative, {10}, AirstrikeMinigame_GateCheck, {self})
  iInsideMinigame = 0
  MrxTutorialManager.HideMessage(false, "VzaCon001")
end

function AirstrikeMinigame_SuccessVO(self)
  MrxVoSequence.Start({
    "Fiona-In-Mission-Contract-Vz01-163"
  })
end

function AirstrikeMinigame_InsideVO(self)
  MrxVoSequence.Start({
    "Fiona-In-Mission-Contract-Vz01-213"
  })
end

function AirstrikeMinigame_GateCheck(self)
  Debug.Printf("*********************************************************************** VZACON001: Printing out values referenced within GateCheck")
  Debug.Printf("*********************************************************************** Igate=" .. iGate)
  if iGate == 1 then
    uGate = "vza001_gate"
  else
    uGate = "vza001_gate2"
  end
  Debug.Printf("*********************************************************************** Ugate=" .. uGate)
  if Object.IsAlive(Pg.GetGuidByName(uGate)) then
    local tCues = {
      "Fiona-In-Mission-Contract-Vz01-170",
      "Fiona-In-Mission-Contract-Vz01-171",
      "Fiona-In-Mission-Contract-Vz01-172",
      "Fiona-In-Mission-Contract-Vz01-173",
      "Fiona-In-Mission-Contract-Vz01-174"
    }
    MrxVoSequence.Start({
      {
        MrxUtil.GetRandomTableElement(tCues),
        uChar
      }
    })
    MrxSupport.PlayRandomVOCue(tVO_GateNag)
    AddFreebies(self)
    ShowGateTutorial(self)
    self:_CreateEvent(Event.ScriptEvent, {
      "Satellite Targetting Start",
      function()
        return true
      end
    }, AirstrikeMinigame, {self})
  end
end

function AirstrikeMinigame_Cancel(self)
  self:_CreateEvent(Event.ScriptEvent, {
    "Satellite Targetting Start",
    function()
      return true
    end
  }, AirstrikeMinigame, {self})
  iInsideMinigame = 0
  Hud.Satellite:SetTutorialText({sText = nil})
  self:_CreateEvent(Event.TimerRelative, {5}, ShowGateTutorial, {self})
end

function TankSwitch(self)
  Debug.Printf("*********************************************************************** VZACON001: TANKSWITCH CALLED!")
  if MrxPlayer.IsInVehicle("Tank && !APC") then
    MrxTutorialManager.ShowMessage("[Tutorial.SwitchWeapons]", false, "VzaCon001")
    self:_CreateEvent(Event.TimerRelative, {7}, MrxTutorialManager.HideMessage, {false, "VzaCon001"})
  end
end

function HandbrakeTutorial(self)
  if MrxPlayer.IsInVehicle("Vehicle") then
    if Gui.ControllerInUse and Gui.ControllerInUse() then
      MrxTutorialManager.ShowMessage("[Tutorial.Handbrake]", false, "VzaCon001")
    else
      MrxTutorialManager.ShowMessage("[SHELL.PCShell.Tutorial_Handbrake_PC]", false, "VzaCon001")
    end
    self:_CreateEvent(Event.TimerRelative, {5}, MrxTutorialManager.HideMessage, {false, "VzaCon001"})
  end
end

function AddFreebies(self)
  MrxSupportData.AddFreebie("VzaCon01_SatBomb", 1, nil, nil, 1)
end

function StopTheMusic(self)
  MrxMusic.StopSpecialMusic()
end

function TreeAttack(self)
  uVehicle = Pg.GetGuidByName("vza_treeattack_1")
  uPath = Pg.GetGuidByName("vza_treehillpath_2")
  local uDriver = Vehicle.GetDriver(uVehicle)
  local moveGoal = Ai.Goal({
    AIGuid = uDriver,
    Goal = "PathMove",
    Target = uPath,
    Haste = 1,
    Priority = "HiPri",
    Timeout = 0
  })
end

function BeachBypass(self)
  oGetToBeach:Complete()
end

function Cleanup(self)
  local tLayersToRemove = {
    "Vz_State_VzaCon001",
    "vz_state_VzaCon001_Pristine",
    "Vz_State_VzaCon001_CP01",
    "Vz_State_VzaCon001_CP02"
  }
  MrxLayerManager.Remove(tLayersToRemove)
  tLayersToAdd = {
    "vz_state_vzacon001_ruined"
  }
  MrxLayerManager.Add(tLayersToAdd)
  MrxSupportData.RemoveFreebie("VzaCon001_Airstrike")
  MrxSupportData.RemoveFreebie("VzaCon01_SatBomb")
  MrxTaskContract.Cleanup(self)
  MrxFactionManager.DisableReporting(false)
  MrxTutorialManager.HideMessage(false, "VzaCon001")
end

function SetMissionMusic(self)
  Debug.Printf("*********************************************************************** VZACON001: SetupMusic")
  Sound.SetDynamicMusic(false)
  MrxMusic.PlaySpecialMusic("mu_nomission_water_explore_01")
end

function VO_GateNag(Self)
  local tVO_GateNag = {
    "Fiona-In-Mission-Contract-Vz01-164",
    "Fiona-In-Mission-Contract-Vz01-165",
    "Fiona-In-Mission-Contract-Vz01-166"
  }
  MrxSupport.PlayRandomVOCue(tVO_GateNag)
end

function PlayHillMusic(self)
  MrxMusic.StopSpecialMusic()
  self:_CreateEvent(Event.TimerRelative, {3}, function()
    MrxMusic.PlaySpecialMusic("Mu_nomission_jungle_threat_02")
  end)
end

function FirstCheckpoint(self)
  if self:_GetFlag("VZ001CP01") then
  else
    self:_SetFlag("VZ001CP01")
    _Checkpoint({"VZACP01_P1", "VZACP01_P2"})
  end
end

function SecondCheckpoint(self)
  if self:_GetFlag("VZ001CP02") then
  else
    self:_SetFlag("VZ001CP02")
    _Checkpoint({"VZACP02_P1", "VZACP02_P2"})
  end
end
