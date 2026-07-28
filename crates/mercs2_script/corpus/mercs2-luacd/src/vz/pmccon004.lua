inherit("MrxTaskContract")
import("MrxApcDrop")
import("MrxUtil")
import("MrxPlayer")
import("MrxTimer")
import("MrxSubtitle")
import("HijackContractManager")
import("MrxSupportData")
import("MrxVoSequence")
import("MrxArtilleryAttack")
import("MrxMusic")
import("MrxFactionManager")
import("MrxVerifyManager")
NETEVENT_HIJACKSOLANO = 0
NETEVENT_ARTILLERYATTACK = 1
NETEVENT_KILLBRIDGE = 2
NETEVENT_CHANGEATMOSPHERE = 3

function LoadAssets(self, tSaveData)
  local tLayersToRemove = {
    "vz_state_SolBunkerBase_Act1",
    "vz_state_SolBunkerBase_Act1",
    "vz_state_PmcCon003_SolBunkerBanse ",
    "vz_state_sol_bunker"
  }
  MrxLayerManager.Remove(tLayersToRemove, function()
    local tLayersToAdd = {
      "vz_state_SolBunkerBase_PmcCon004",
      "vz_state_pmccon004",
      "vz_state_sol_base_pristine",
      "vz_state_SolanoBase_PMC004",
      "vz_state_sol_bunker_pmc004",
      "vz_state_Car_city_pristine"
    }
    MrxLayerManager.Add(tLayersToAdd, self.AssetsLoaded, {self})
  end)
end

function Activated(self)
  MrxTaskContract.Activated(self)
  Graphics.Atmosphere.ChangeLineRegionSetting(Pg.GetGuidByName("rgn_atmo_caracas"), "warzone")
  Graphics.Atmosphere.ChangeLineRegionSetting(Pg.GetGuidByName("rgn_atmo_Angelfalls"), "WarzoneSolano")
  if Net.IsServer() then
    Net.SendCustomEvent("PmcCon004", NETEVENT_CHANGEATMOSPHERE, {})
    _evClientJoinedPMC004 = Event.CreatePersistent(Event.ScriptEvent, {
      "mpPlayerJoin",
      function(tData)
        return Net.IsServer() and not Player.IsLocal(tData[1])
      end
    }, Net.SendCustomEvent, {
      "PmcCon004",
      NETEVENT_CHANGEATMOSPHERE,
      {}
    })
  end
  Net.IsServer()
  local uVehicle = Pg.GetGuidByName("Alouette3 Attack (VZ) 0x00105219")
  self:_CreateEvent(Event.ObjectHibernation, {uVehicle, "awake"}, Vehicle.Usable, {uVehicle, false})
  self:CreateChild({
    sName = "DestroyBunker",
    sModuleName = "MrxTaskObjectiveDestroy",
    vTgtInclude = "Solano_Bunker_PMC004",
    sDspShortDesc = "[PmcCon004.Objectives.001]",
    fOnComplete = function()
      self:_SetFlag("HijackInitiated")
      _Checkpoint({
        "BunkerCheckpoint"
      })
      self:_CreateEvent(Event.TimerRelative, {7}, function()
        MrxVoSequence.Start({
          "Fiona.Cam.02"
        })
      end)
      self:SolanoHijackInit()
    end,
    tOnCancel = {
      {
        self.BunkerIntact,
        {self}
      }
    }
  })
  MrxSupportData.AddFreebie("PmcCon004_Nuke")
  self:_CreateEvent(Event.ScriptEvent, {
    "Nuked",
    function()
      return true
    end
  }, NukeDetonated, {self})
  SetCompoundMusic(self)
  BridgeDestruction(self)
  HijackContractManager.SetActiveContract(self)
  if not self:_GetFlag("HijackInitiated") then
    MrxVoSequence.Start({
      "Fiona-In-Mission-Contract-Pmc04-01",
      0.5,
      "Misha-In-Mission-Contract-Pmc04-02",
      0.5,
      "Fiona-In-Mission-Contract-Pmc04-03",
      0.5,
      {
        SetMissionMusic,
        {self}
      }
    })
  end
end

function NukeDetonated(self, tLoc)
  Debug.Printf("*************************************** PMCCON004: NUKE EVENT FIRED")
  if MrxUtil.GetDistanceToObject("Solano_Bunker_PMC004", tLoc[1], tLoc[2], tLoc[3], true) < 20 then
    Debug.Printf("*********************************************************************** PMCCON004: Destroying Bunker")
    Object.Kill(Pg.GetGuidByName("Solano_Bunker_PMC004"))
    local player = Player.GetLocalPlayer()
    local playerCamera = Player.GetCamera(player)
    local playerCharacter = Player.GetCharacter(player)
    Camera.Shake(playerCamera, "ShakeCameraMedium", playerCharacter, 6, 5)
  else
    MrxVoSequence.Start({
      "Fiona-In-Mission-Contract-Pmc04-65",
      3,
      {
        self.BunkerIntact,
        {self}
      }
    })
  end
end

function SetMissionMusic(self)
  Debug.Printf("*********************************************************************** PMCCON004: SetMissionMusic")
  Sound.SetDynamicMusic(false)
  MrxMusic.PlaySpecialMusic("mu_pmc_006_01")
end

function SetCompoundMusic(self)
  Debug.Printf("*********************************************************************** PMCCON004: SET COMPOUND MUSIC")
  self:_CreateEvent(Event.Boundary, {
    Player.GetAnyCharacter(),
    Pg.GetGuidByName("lnrgn_CompoundMusic"),
    "enter",
    false
  }, function()
    Debug.Printf("*********************************************************************** PMCCON004 MUSIC BOUNDARY TRIGGERED")
    MrxMusic.StopSpecialMusic()
    local boundaryMusic = self:_CreateEvent(Event.TimerRelative, {3}, function()
      MrxMusic.PlaySpecialMusic("mu_pmc_006_02")
    end)
  end)
end

function BridgeDestruction(self)
  self:_CreateEvent(Event.Boundary, {
    Player.GetAnyCharacter(),
    Pg.GetGuidByName("lnrg_destroybridge1"),
    "enter",
    false
  }, function()
    Debug.Printf("********************* PMCCON004:  DestroyBridge1 Destroyed")
    MrxVoSequence.Start({
      "Solano.vp7sol01",
      0.5,
      {
        mattias = "Mattias-In-Mission-Contract-Pmc04-06",
        jennifer = "Jennifer-In-Mission-Contract-Pmc04-07",
        chris = "Chris-In-Mission-Contract-Pmc04-08"
      }
    })
    MrxArtilleryAttack.Create(Pg.GetGuidByName("loc_solattack1", 16))
    MrxArtilleryAttack.Create(Pg.GetGuidByName("loc_solattack2", 16))
    Net.SendCustomEvent("PmcCon004", NETEVENT_ARTILLERYATTACK, {
      Pg.GetGuidByName("loc_solattack1", 16),
      Pg.GetGuidByName("loc_solattack2", 15)
    })
    local destroyBridge1 = self:_CreateEvent(Event.TimerRelative, {10}, function()
    end)
  end)
  self:_CreateEvent(Event.Boundary, {
    Player.GetAnyCharacter(),
    Pg.GetGuidByName("lnrg_destroybridge2"),
    "enter",
    false
  }, function()
    Debug.Printf("********************* PMCCON004: DestroyBridge2 Destroyed")
    MrxVoSequence.Start({
      "Solano.vp7sol02",
      0.5,
      {
        mattias = "Mattias-In-Mission-Contract-Pmc04-67",
        jennifer = "Jennifer-In-Mission-Contract-Pmc04-68",
        chris = "Chris-In-Mission-Contract-Pmc04-69"
      }
    })
    MrxArtilleryAttack.Create(Pg.GetGuidByName("loc_solattack3", 16))
    MrxArtilleryAttack.Create(Pg.GetGuidByName("loc_solattack4", 16))
    Net.SendCustomEvent("PmcCon004", NETEVENT_ARTILLERYATTACK, {
      Pg.GetGuidByName("loc_solattack3", 16),
      Pg.GetGuidByName("loc_solattack4", 15)
    })
    local destroyBridge1 = self:_CreateEvent(Event.TimerRelative, {10}, function()
    end)
  end)
end

function DetectBridgeOneProximity(self)
  Debug.Printf("********************* PMCCON004: BRIDGE ONE ENTERED")
  self:_CreateEvent(Event.Boundary, {
    Player.GetAnyCharacter(),
    Pg.GetGuidByName("lnrgn_apcdrop1"),
    "enter",
    false
  }, SetUpApcDrop1({self}))
end

function DetectBridgeTwoProximity(self)
  Debug.Printf("********************* PMCCON004: BRIDGE TWO ENTERED")
  self:_CreateEvent(Event.Boundary, {
    Player.GetAnyCharacter(),
    Pg.GetGuidByName("lnrgn_apcdrop2"),
    "enter",
    false
  }, SetUpApcDrop2, {self})
end

function DetectBridgeThreeProximity(self)
  Debug.Printf("********************* PMCCON004: BRIDGE THREE ENTERED")
  self:_CreateEvent(Event.Boundary, {
    Player.GetAnyCharacter(),
    Pg.GetGuidByName("lnrgn_apcdrop3"),
    "enter",
    false
  }, SetUpApcDrop3, {self})
end

function SetUpApcDrop1(self)
  Debug.Printf("********************* PMCCON004: SETUP APCDROP1")
  ApcDrop(self, "pth_apcdrop1_veh1", "M151 .50Cal (VZ) (Full)")
  ApcDrop(self, "pth_apcdrop1_veh2", "M151 .50Cal (VZ) (Full)")
end

function SetUpApcDrop2(self)
  Debug.Printf("********************* PMCCON004: SETUP APCDROP2")
  ApcDrop(self, "pth_apcdrop2_veh1", "M35 (Cargo) (VZ) (Full)")
  ApcDrop(self, "pth_apcdrop2_veh2", "M35 (Cargo) (VZ) (Full)")
end

function SetUpApcDrop3(self)
  Debug.Printf("********************* PMCCON004: SETUP APCDROP3")
  ApcDrop(self, "pth_apcdrop3_veh1", "M113 (VZ) (Full)")
  ApcDrop(self, "pth_apcdrop3_veh2", "M113 (VZ) (Full)")
end

function ApcDrop(self, sPath, sVeh)
  Debug.Printf("********************* PMCCON004: APCDROP")
  local res, x, y, z, yaw = MrxUtil.FindSpawnPointOutOfView(Pg.GetGuidByName(sPath), 200)
  if not res then
    Debug.Printf("BAD RESULT FROM SPAWN POINT? ")
    return
  end
  local apc = Pg.Spawn(sVeh, x, y, z, yaw)
  Object.SetHibernationDistance(apc, 300)
  self:_CreateEvent(Event.ObjectHibernation, {apc, "awake"}, function()
    local tApcDropData = {
      uVehicle = apc,
      inDest = sPath,
      inDestType = "path",
      inSpeed = 0.7
    }
    local e = self:_CreateEvent(Event.TimerRelative, {3}, function(tData)
      MrxApcDrop:Create(tData)
    end, {tApcDropData})
  end)
end

function _DelayedAPCSpawn(self, aDeadAPC)
  if aDeadAPC then
    Debug.Printf("SPAWNING BECAUSE APC WAS DESTROYED")
    gtAPCEvents[aDeadAPC] = nil
  end
  local spawnTime = Math.randi(kfAPCSpawnDelayLow, kfAPCSpawnDelayHigh)
  self:_CreateEvent(Event.TimerRelative, {spawnTime}, _APCSpawn, {self})
end

function DetectHeli1(self)
  Debug.Printf("********************* PMCCON004: HELI AREA ONE ENTERED")
  self:_CreateEvent(Event.Boundary, {
    Player.GetAnyCharacter(),
    Pg.GetGuidByName("lnrgn_helicheck1"),
    "enter",
    false
  }, SpawnHelis, {self})
end

function DetectHeli2(self)
  Debug.Printf("********************* PMCCON004: HELI AREA TWO ENTERED")
  self:_CreateEvent(Event.Boundary, {
    Player.GetAnyCharacter(),
    Pg.GetGuidByName("lnrgn_helicheck2"),
    "enter",
    false
  }, SpawnHelis, {self})
end

function SpawnHelis(self)
  Debug.Printf("********************* PMCCON004: SpawnHelis")
  if MrxPlayer.IsInVehicle("Helicopter") then
    local uX, uY, uZ = Object.GetPosition(Player.GetCharacter(Player.GetLocalPlayer()))
    Pg.Spawn("Alouette3 Superiority (Driver)", uX + 100, uY + 100, uZ + 100)
  end
end

function SolanoHijackInit(self)
  Debug.Printf("********************* PMCCON004: SolanoHijackInit")
  MrxLayerManager.Add("vz_State_PmcCon004SolanoHijack", SolanoHijackLayerLoaded, {self})
  self:_CreateEvent(Event.ScriptEvent, {
    "SolanoHijackComplete",
    function()
      Debug.Printf("********************* PmcCon004: Solano Hijack Completed")
      return true
    end
  }, self.Complete, {self})
  self:_CreateEvent(Event.ScriptEvent, {
    "SolanoHijackFailed",
    function()
      Debug.Printf("********************* PmcCon004: Solano Hijack Failed")
      return true
    end
  }, self.Cancel, {self})
end

function SolanoObjective(self)
  Debug.Printf("********************* PMCCON004: SolanoObjective")
  self:CreateChild({
    sName = "Get Solano",
    sModuleName = "MrxTaskObjectiveDeliver",
    vTgtInclude = Player.GetAnyCharacter(),
    vDestLoc = "loc_BunkerDoor",
    vDestRegion = "lnrgn_GetSolano",
    sDspShortDesc = "[PmcCon004.Objectives.003]",
    tOnComplete = {
      {
        SolanoHijackObjective,
        {self}
      }
    },
    tOnCancel = {
      {
        self.SolanoEscaped,
        {self}
      }
    }
  })
end

function SolanoHijackObjective(self)
  Debug.Printf("********************* PMCCON004: SolanoObjective")
  oSolanoHeliObjective = self:CreateChild({
    sName = "Get Solano",
    sModuleName = "MrxTaskObjectiveEnterVehicle",
    vTgtInclude = "Mi35 (Solano Hijack)",
    nQuota = 1,
    uPlayer = Player.GetAnyCharacter(),
    sDspShortDesc = "[PmcCon004.Objectives.002]",
    tOnComplete = {},
    tOnCancel = {}
  })
end

function SolanoHijackLayerLoaded(self)
  SolanoObjective(self)
  PlayerEntersDockRegion(self)
  SolanoStaging(self)
end

function SolanoStaging(self)
  self:_CreateEvent(Event.Boundary, {
    Player.GetAnyCharacter(),
    Pg.GetGuidByName("lnrgn_explosion1"),
    "enter",
    false
  }, function()
    MrxUtil.SpawnObject("Explosion (grenade)", Pg.GetGuidByName("loc_explosion1"))
  end)
  self:_CreateEvent(Event.Boundary, {
    Player.GetAnyCharacter(),
    Pg.GetGuidByName("lnrgn_explosion2_3"),
    "enter",
    false
  }, function()
    MrxUtil.SpawnObject("Explosion (grenade)", Pg.GetGuidByName("loc_explosion2"))
    MrxUtil.SpawnObject("Explosion (grenade)", Pg.GetGuidByName("loc_explosion3"))
    local uMook1 = MrxUtil.SpawnObject("VZ Soldier (Mook)", Pg.GetGuidByName("loc_spawnmans"))
    local uMook2 = MrxUtil.SpawnObject("VZ Soldier (Mook)", Pg.GetGuidByName("loc_spawnmans"))
    Ai.Goal({
      AIGuid = uMook1,
      Goal = "PathMove",
      Target = Pg.GetGuidByName("pth_explodingmans"),
      Priority = "HiPri",
      Haste = 1
    })
    Ai.Goal({
      AIGuid = uMook2,
      Goal = "PathMove",
      Target = Pg.GetGuidByName("pth_explodingmans"),
      Priority = "HiPri",
      Haste = 1
    })
  end)
end

function PlayerEntersTimerRegion(self)
  Debug.Printf("********************* PMCCON004: PlayerEntersTimerRegion")
  self:_CreateEvent(Event.Boundary, {
    Player.GetAnyCharacter(),
    Pg.GetGuidByName("timerLineRegion"),
    "enter",
    false
  }, SolanoRace, {self})
  Debug.Printf("********************* Timer REGION ACTIVE ")
end

function PlayerEntersDockRegion(self)
  Debug.Printf("********************* PMCCON004: PlayerEntersDockRegion")
  Event.Create(Event.ObjectProximity, {
    "hero",
    Pg.GetGuidByName("activateActionHijack_Lineregion"),
    "<",
    50
  }, ProximityCallback, {self})
end

function OpenBunkerDoors(self)
  local uBunkerDoorGuid = Pg.GetGuidByName("SolanoBunkerDoors")
  Object.OpenGate(uBunkerDoorGuid)
end

function ProximityCallback(self, tGuid)
  Debug.Printf("********************* PMCCON004: ProximityCallback")
  uCharGuid = tGuid[1]
  Debug.Printf("********************* PMCCON004: tGuid")
  Debug.Printf(uCharGuid)
  uLocalChar = Player.GetLocalCharacter()
  Debug.Printf("********************* PMCCON004: tGuid")
  Debug.Printf(uLocalChar)
  if uLocalChar == uCharGuid then
    DockAttack(self, uCharGuid)
  else
    Net.SendCustomEvent("PmcCon004", NETEVENT_HIJACKSOLANO, {})
  end
end

function NetEventCallback(nEventType, tArgs)
  if nEventType == NETEVENT_HIJACKSOLANO then
    DockAttack(self, Player.GetLocalCharacter())
  elseif nEventType == NETEVENT_ARTILLERYATTACK then
    MrxArtilleryAttack.Create(tArgs[1])
    MrxArtilleryAttack.Create(tArgs[2])
  elseif nEventType == NETEVENT_KILLBRIDGE then
    Object.Kill(tArgs[1])
    Object.Kill(tArgs[2])
  elseif nEventType == NETEVENT_CHANGEATMOSPHERE then
    Graphics.Atmosphere.ChangeLineRegionSetting(Pg.GetGuidByName("rgn_atmo_caracas"), "warzone")
    Graphics.Atmosphere.ChangeLineRegionSetting(Pg.GetGuidByName("rgn_atmo_Angelfalls"), "WarzoneSolano")
  end
end

function DockAttack(self, tGuid)
  Debug.Printf("*******************PMCCON004: DockAttack")
  if oTimer then
    oTimer:Stop()
  end
  uCurrentVehicle = Vehicle.GetFromRider(tGuid)
  if uCurrentVehicle then
    Vehicle.Exit(uCurrentVehicle, tGuid, true)
  end
  local c = Pg.GetGuidByName("Mi35 (Solano Hijack)")
  Event.Create(Event.TimerRelative, {0.5}, function()
    local d = Vehicle.Enter(c, tGuid, "d", false, true)
  end)
end

function SolanoRace(self)
  Debug.Printf("*******************SOLANO RACE: ENTERED LINE REGION FOR TIMER")
  oTimer = MrxTimer:Create({
    nStartTime = 60,
    nWarning = 15,
    iTray = 2,
    tDoneCallbacks = {
      {
        OutOfTimeSubtitle,
        {self}
      }
    }
  })
  StartTimeSubtitle(self)
  oTimer:Start()
end

function StartTimeSubtitle(self)
  Debug.Printf("*******************StartTimeSubtitle")
  self:_PlayVo(0, "Fiona-In-Mission-Contract-Pmc04-10")
end

function OutOfTimeSubtitle(self)
  Debug.Printf("*******************OutOfTimeSubtitle")
  HijackContractManager.CancelActiveContract()
end

function BunkerIntact(self)
  self:_SetCancelMessage("[PmcCon004.Terms.Cancel01]")
  self:Cancel()
end

function SolanoEscaped(self)
  self:_SetCancelMessage("[PmcCon004.Terms.Cancel02]")
  MrxVoSequence.Start({
    "Fiona-In-Mission-Contract-Pmc04-66",
    3,
    {
      self.Cancel,
      {self}
    }
  })
end

function Cleanup(self)
  Event.Delete(_evClientJoinedPMC004)
  if MrxVerifyManager.GetKilled() == 0 then
    if Net.IsActive() then
      self:_SetPlayer1Bonus(25000000)
      self:_SetPlayer2Bonus(25000000)
    else
      self:_SetPlayer1Bonus(25000000)
    end
  end
  MrxSupportData.RemoveFreebie("PmcCon004_Nuke")
  MrxTaskContract.Cleanup(self)
end
