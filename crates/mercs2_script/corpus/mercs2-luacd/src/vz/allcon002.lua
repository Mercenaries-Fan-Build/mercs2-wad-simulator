inherit("MrxTaskContract")
import("DangerousBuilding")
import("MrxSubtitle")
import("MrxVoSequence")
import("MrxSupportData")
import("MrxFactionManager")
NETEVENT_STARTEMITTERS = 0
NETEVENT_STARTPLUMES = 1
NETEVENT_CLEANSMOKE = 2
NETEVENT_AIRSTRUCK = 3

function LoadAssets(self, tSaveData)
  local tLayersToRemove = {
    "vz_state_car_shanty_act1",
    "vz_state_staging_all_HQ",
    "vz_state_car_city_act1"
  }
  local tLayersToAdd = {
    "Vz_State_AllCon002"
  }
  if self:_GetFlag("BoatsKilled") then
    table.insert(tLayersToRemove, "vz_State_AllCon002_Boats")
    table.insert(tLayersToRemove, "vz_State_AllCon002_mlrs")
    table.insert(tLayersToAdd, "vz_State_AllCon002_officers")
  elseif self:_GetFlag("MLRSkilled") then
    table.insert(tLayersToRemove, "vz_State_AllCon002_mlrs")
    table.insert(tLayersToAdd, "vz_State_AllCon002_Boats")
  else
    table.insert(tLayersToAdd, "vz_State_AllCon002_mlrs")
  end
  MrxLayerManager.Remove(tLayersToRemove, function()
    MrxLayerManager.Add(tLayersToAdd, self.AssetsLoaded, {self})
  end)
end

function NetEventCallback(eventId, tArgs)
  if eventId == NETEVENT_STARTEMITTERS then
    Pop(tArgs[1], tArgs[2], tArgs[3], tArgs[4])
  elseif eventId == NETEVENT_STARTPLUMES then
    Plumes(tArgs[1], tArgs[2], tArgs[3], tArgs[4])
  elseif eventId == NETEVENT_CLEANSMOKE then
    SmokeClean()
  elseif eventId == NETEVENT_AIRSTRUCK then
    AirStriked(tArgs[1], tArgs[2], tArgs[3])
  end
end

function Activated(self)
  _tSmokeParticleObjects = {}
  MrxTaskContract.Activated(self)
  nStrike = 0
  nCaracasLife = 100
  nAAalive = 3
  bBombard = 1
  nStrikeFrequency = 1
  uANTalk = 4
  uShellVoPlayed = 0
  nDamMod = 2
  Flybys(self)
  self:_CreateEvent(Event.TimerRelative, {15}, SetupANTalker, {self})
  if self:_GetFlag("BoatsKilled") then
    Obj4_Verify(self)
  elseif self:_GetFlag("MLRSkilled") then
    nCaracasLife = self:_GetFlag("MLRSkilled")
    DisplayCaracasHealth(self)
    MrxVoSequence.Start({
      "Fiona-In-Mission-Contract-All02-28",
      {
        Obj3_BoatAA,
        {self}
      }
    })
  else
    DestroyMLRS(self)
  end
end

function DestroyMLRS(self)
  local tStartsVO = {
    "Fiona-Banter-Contract-All01-01",
    {
      mattias = "mattias-Banter-Contract-All01-28",
      jennifer = "jennifer-Banter-Contract-All01-30",
      chris = "chris-Banter-Contract-All01-29"
    },
    "Fiona-In-Mission-Contract-All02-22"
  }
  self:CreateChild({
    sName = "DestroyChinaAA",
    sModuleName = "MrxTaskObjectiveDestroy",
    vTgtInclude = {
      "Obj1_A",
      "Obj1_B",
      "Obj1_C"
    },
    vVoSeqOnAdd = tStartsVO,
    sDspShortDesc = "[AllCon002.Objectives.001]",
    tOnPartComplete = {
      {
        AADestroyed,
        {self}
      }
    },
    fOnComplete = function()
      self:_SetFlag("MLRSkilled", nCaracasLife)
      _Checkpoint({
        "Loc_All002_Ckpt_1p1",
        "Loc_All002_Ckpt_1p2"
      })
      MrxLayerManager.Add({
        "vz_State_AllCon002_Boats"
      })
      MrxVoSequence.Start({
        "Fiona-In-Mission-Contract-All02-28",
        {
          Obj3_BoatAA,
          {self}
        }
      })
    end,
    tOnCancel = {
      {
        self.Cancel,
        {self}
      }
    }
  })
  DisplayCaracasHealth(self)
  self:_CreateEvent(Event.TimerRelative, {5}, Obj1_Bombard, {self, "A"})
  self:_CreateEvent(Event.TimerRelative, {10}, Obj1_Bombard, {self, "B"})
  self:_CreateEvent(Event.TimerRelative, {20}, Obj1_Bombard, {self, "C"})
  local uLauncher = Pg.GetGuidByName("Obj1_A")
  Event.Create(Event.ObjectPhysicsEvent, {
    uLauncher,
    "VehicleSinking"
  }, LauncherSunk, {self, uLauncher})
  local uLauncher = Pg.GetGuidByName("Obj1_B")
  Event.Create(Event.ObjectPhysicsEvent, {
    uLauncher,
    "VehicleSinking"
  }, LauncherSunk, {self, uLauncher})
  local uLauncher = Pg.GetGuidByName("Obj1_C")
  Event.Create(Event.ObjectPhysicsEvent, {
    uLauncher,
    "VehicleSinking"
  }, LauncherSunk, {self, uLauncher})
  local uPlayerOoops = Player.GetAnyCharacter()
  for i = 1, 5 do
    local uStrikeLoc = Pg.GetGuidByName("loc_Rockets_" .. i)
    local uSeeStrike = Pg.GetGuidByName("loc_Rockets_see_" .. i)
    self:_CreateEvent(Event.ObjectProximity, {
      uPlayerOoops,
      uSeeStrike,
      "<",
      65,
      false,
      true
    }, MPShelling, {
      uStrikeLoc,
      uSeeStrike,
      i
    })
  end
  eAirstrikes = self:_CreateEvent(Event.Boundary, {
    Player.GetPrimaryCharacter(),
    Pg.GetGuidByName("Reg_AllCon002_Strikes"),
    "enter",
    false
  }, AirstrikesOn, {self})
end

function LauncherSunk(self, uSinkingVeh)
  self:_CreateEvent(Event.TimerRelative, {5}, Object.Kill, {uSinkingVeh})
end

function Obj3_BoatAA(self)
  bBombard = 2
  nDamMod = 1
  nHuangs = 3
  eDamageCarac = self:_CreatePersistentEvent(Event.TimerRelative, {30}, DamageCaracas, {self})
  if eAirstrikes then
    Event.Delete(eAirstrikes)
  end
  if eAirstrikes2 then
    Event.Delete(eAirstrikes2)
  end
  self:_CreateEvent(Event.TimerRelative, {2}, SetupWreck, {self})
  self:CreateChild({
    sName = "DestroyBoatChinaAA",
    sModuleName = "MrxTaskObjectiveDestroy",
    vTgtInclude = {
      "RiverBoat_1",
      "RiverBoat_2",
      "RiverBoat_3"
    },
    sDspShortDesc = "[AllCon002.Objectives.002]",
    tOnPartComplete = {
      {
        BoatDestroyed,
        {self}
      }
    },
    fOnComplete = function()
      self:_SetFlag("BoatsKilled")
      _Checkpoint({
        "Loc_All2_Check2p1",
        "Loc_All2_Check2p2"
      })
      MrxLayerManager.Add({
        "vz_State_AllCon002_officers"
      })
      MrxVoSequence.Start({
        "Fiona-In-Mission-Contract-All02-37",
        "Fiona-In-Mission-Contract-All02-39",
        {
          Obj4_Verify,
          {self}
        }
      })
    end,
    tOnCancel = {
      {
        self.Cancel,
        {self}
      }
    }
  })
end

function BoatDestroyed(self)
  nHuangs = nHuangs - 1
  if nHuangs == 2 then
    MrxVoSequence.Start({
      "Fiona-In-Mission-Contract-All01-23"
    })
  elseif nHuangs == 1 then
    MrxVoSequence.Start({
      "Fiona-In-Mission-MinorContract-Oil04-09"
    })
  end
end

function Obj4_Verify(self)
  Ai.Role({
    AIGuid = Pg.GetGuidByName("Turncoat"),
    Role = "Idle",
    Priority = "hiPri"
  })
  self:_CreateEvent(Event.ObjectProximity, {
    Player.GetAnyCharacter(),
    Pg.GetGuidByName("Turncoat"),
    "<",
    35,
    false,
    true
  }, RunAway, {self, 1})
  if eBoatDamage then
    Event.Delete(eBoatDamage)
  end
  if eDamageCarac then
    Event.Delete(eDamageCarac)
  end
  if eDisplay1 then
    Event.Delete(eDisplay1)
  end
  if eDisplay2 then
    Event.Delete(eDisplay2)
  end
  if eDisplay3 then
    Event.Delete(eDisplay3)
  end
  nOfficers = 3
  Hud.ObjectiveTray:ClearSlot({vPlayer = nil, nSlot = 1})
  Hud.ObjectiveTray:ClearSlot({vPlayer = nil, nSlot = 2})
  self:CreateChild({
    sName = "Verify the Allied turncoat",
    sModuleName = "MrxTaskObjectiveDestroy",
    vTgtInclude = {
      "Turncoat",
      "Turncoat2",
      "Turncoat3"
    },
    sDspShortDesc = "[AllCon002.Objectives.003]",
    tOnPartComplete = {
      {
        OfficerDown,
        {self}
      }
    },
    fOnComplete = function()
      MrxVoSequence.Start({
        "Fiona-In-Mission-Contract-All02-40",
        "Fiona-In-Mission-Contract-All02-41",
        {
          self.Complete,
          {self}
        }
      })
    end
  })
end

function OfficerDown(self)
  nOfficers = nOfficers - 1
  if nOfficers == 2 then
    MrxVoSequence.Start({
      "Fiona-In-Mission-Contract-All02-38"
    })
  elseif nOfficers == 1 then
    MrxVoSequence.Start({
      "Fiona-In-Mission-Contract-All02-36"
    })
  end
end

function RunAway(self, nSegment)
  if 1 < nSegment then
    Event.Delete(uSpot)
    Event.Delete(uDistSpot)
    Event.Delete(uCloseSpot)
  end
  Ai.Goal({
    AIGuid = Pg.GetGuidByName("Turncoat"),
    Goal = "PathMove",
    Target = Pg.GetGuidByName("Pa_RunMan_" .. nSegment),
    Haste = 0.8,
    Priority = "hiPri",
    Callback = RunAwayCheck,
    CallbackData = {self, nSegment}
  })
end

function RunAwayCheck(self, nSegment, Guid, State)
  if State == 0 then
    RunAway(self, nSegment)
  elseif State == 1 then
    local nSegment = nSegment + 1
    if nSegment == 6 then
    else
      uDistSpot = self:_CreateEvent(Event.ObjectProximity, {
        Player.GetAnyCharacter(),
        Pg.GetGuidByName("Turncoat"),
        "<",
        20,
        false,
        true
      }, SpottedHim, {self, nSegment})
      uCloseSpot = self:_CreateEvent(Event.ObjectProximity, {
        Player.GetAnyCharacter(),
        Pg.GetGuidByName("Turncoat"),
        "<",
        5,
        false,
        true
      }, RunAway, {self, nSegment})
    end
  end
end

function SpottedHim(self, nRunAway)
  if Object.IsVisible(Pg.GetGuidByName("Turncoat")) then
    RunAway(self, nRunAway)
  else
    uSpot = self:_CreateEvent(Event.TimerRelative, {2}, SpottedHim, {self, nRunAway})
  end
end

function AADestroyed(self)
  nAAalive = nAAalive - 1
  if nAAalive == 2 then
    MrxVoSequence.Start({
      "Fiona-In-Mission-Contract-All02-29"
    })
  elseif nAAalive == 1 then
    MrxVoSequence.Start({
      "Fiona-In-Mission-Contract-All02-30"
    })
  elseif nAAalive == 0 then
  end
end

function AlliedSpeaks(self, tAlliedTalk)
  uANTalk = uANTalk - 1
  if uANTalk == 3 then
    MrxVoSequence.Start({
      {
        "AlliedSoldier-In-Mission-Contract-All02-32",
        tAlliedTalk[1]
      }
    })
  elseif uANTalk == 2 then
    MrxVoSequence.Start({
      {
        "AlliedSoldier-In-Mission-Contract-All02-33",
        tAlliedTalk[1]
      }
    })
  elseif uANTalk == 1 then
    MrxVoSequence.Start({
      {
        "AlliedSoldier-In-Mission-Contract-All02-34",
        tAlliedTalk[1]
      }
    })
  else
    uANTalk = 4
  end
  self:_CreateEvent(Event.TimerRelative, {20}, SetupANTalker, {self})
end

function SetupANTalker(self)
  local x, y, z = Object.GetPosition(Player.GetPrimaryCharacter())
  local tANsoldiers = {}
  local tANsoldiers = Pg.FastCollectHumans(x, y, z, 15, "Allied && Human")
  nAnSold = table.getn(tANsoldiers)
  if nAnSold > 0 then
    AlliedSpeaks(self, tANsoldiers)
  else
    self:_CreateEvent(Event.TimerRelative, {15}, SetupANTalker, {self})
  end
end

function AirstrikeThePlayer(self)
  if uAirstrikeOn == 1 then
    MPShelling()
    eAnotherStrike = self:_CreateEvent(Event.TimerRelative, {nStrikeFrequency}, AirstrikeThePlayer, {self})
  end
end

function AirstrikesOff(self)
  uAirstrikeOn = 0
  eAirstrikes2 = self:_CreateEvent(Event.Boundary, {
    Player.GetPrimaryCharacter(),
    Pg.GetGuidByName("Reg_AllCon002_Strikes"),
    "enter",
    false
  }, AirstrikesOn, {self})
end

function AirstrikesOn(self)
  if uShellVoPlayed == 0 then
    uShellVoPlayed = 1
    MrxVoSequence.Start({
      "Fiona-In-Mission-Contract-All02-26"
    })
  end
  uAirstrikeOn = 1
  self:_CreateEvent(Event.Boundary, {
    Player.GetPrimaryCharacter(),
    Pg.GetGuidByName("Reg_AllCon002_Strikes"),
    "exit",
    false
  }, AirstrikesOff, {self})
  AirstrikeThePlayer(self)
end

function Obj1_Bombard(self, sLauncher)
  local uRocketLauncher = Pg.GetGuidByName("Obj1_" .. sLauncher)
  if uRocketLauncher then
    if Object.IsAwake(uRocketLauncher) and not Object.IsPlayerControlled(uRocketLauncher) then
      local uRocketMan = Vehicle.GetDriver(uRocketLauncher)
      if uRocketMan then
        uSeat = Vehicle.GetSeatByType(uRocketLauncher, "p")
        Debug.Printf("*>*>*>*>*>*>*>  Locked out" .. tostring(uSeat))
        Vehicle.Usable(uSeat, false)
        Ai.Anchor({AIGuid = uRocketMan, AnchorRadius = 0})
        Ai.Goal({
          AIGuid = uRocketMan,
          Goal = "Attack",
          Force = true,
          Target = Pg.GetGuidByName("loc_FireTgt_" .. sLauncher),
          Priority = "hiPri"
        })
        if bBombard == 1 then
          self:_CreateEvent(Event.TimerRelative, {5}, Obj1_StopBombard, {self, sLauncher})
          self:_CreateEvent(Event.TimerRelative, {10.5}, ShellCaracas, {self, sLauncher})
        end
      end
    elseif bBombard == 1 then
      self:_CreateEvent(Event.TimerRelative, {10.5}, ShellCaracas, {self, sLauncher})
      self:_CreateEvent(Event.TimerRelative, {26}, Obj1_Bombard, {self, sLauncher})
    end
  end
end

function Obj1_StopBombard(self, sLauncher)
  local uRocketLauncher = Pg.GetGuidByName("Obj1_" .. sLauncher)
  if uRocketLauncher and not Object.IsPlayerControlled(uRocketLauncher) then
    local uRocketMan = Vehicle.GetDriver(uRocketLauncher)
    if uRocketMan then
      Ai.RemoveGoal({AIGuid = uRocketMan, Handle = 0})
      Ai.Goal({
        AIGuid = uRocketMan,
        Goal = "Idle",
        LeaveTurretOn = true,
        Priority = "hiPri"
      })
      if bBombard == 1 then
        self:_CreateEvent(Event.TimerRelative, {26}, Obj1_Bombard, {self, sLauncher})
      end
    end
  end
end

function SetupWreck(self)
  tBridgeSeg = {
    "AllCon002_kill_front",
    "AllCon002_kill_mid",
    "AllCon002_kill_back",
    "_cumana_bridge_midA 0x000caf7c",
    "_cumana_bridge_midA 0x000caf7b"
  }
  for i, uSeg in ipairs(tBridgeSeg) do
    local uPiece = Pg.GetGuidByName(uSeg)
    self:_CreateEvent(Event.ObjectHibernation, {uPiece, "awake"}, WreckBridge, {self, uPiece})
    self:_CreateEvent(Event.TimerRelative, {4}, BoatDelay, {self})
  end
end

function WreckBridge(self, uPiece)
  if uPiece then
    Object.Kill(uPiece)
  end
end

function BoatDelay(self)
  for i = 1, 3 do
    local uBoat = Pg.GetGuidByName("RiverBoat_" .. i)
    self:_CreateEvent(Event.ObjectHibernation, {uBoat, "awake"}, StartRiverAttack, {self, i})
  end
end

function StartRiverAttack(self, nBoat)
  if Object.IsAlive(Pg.GetGuidByName("RiverBoat_" .. nBoat)) then
    local uBoatCapn = Vehicle.GetDriver(Pg.GetGuidByName("RiverBoat_" .. nBoat))
    if uBoatCapn and not Object.IsPlayerControlled(Pg.GetGuidByName("RiverBoat_" .. nBoat)) then
      nBoatTime = nBoat * 4
      self:_CreateEvent(Event.TimerRelative, {nBoatTime}, PathMoveBoat, {
        self,
        nBoat,
        uBoatCapn
      })
    end
  end
end

function PathMoveBoat(self, nBoat, uBoatCapn)
  local uHuang = Pg.GetGuidByName("RiverBoat_" .. nBoat)
  if uHuang then
    local uBoatCapn = Vehicle.GetDriver(uHuang)
    if uBoatCapn and not Object.IsPlayerControlled(Pg.GetGuidByName("RiverBoat_" .. nBoat)) then
      Debug.Printf("Moving on For " .. nBoat .. " Boat %%%%%%%%%%%")
      Ai.Goal({
        AIGuid = uBoatCapn,
        Goal = "PathMove",
        Target = Pg.GetGuidByName("Pa_Boat_" .. nBoat .. "_1"),
        Start = "Nearest",
        Priority = "hiPri",
        Force = true,
        Callback = RiverAttack,
        CallbackData = {self, nBoat}
      })
    end
  end
end

function RiverAttack(self, nBoat, uBoatCapn, State)
  if State == 0 then
    Debug.Printf(nBoat .. "Didn't make the path !!!!!  Try again!!")
    PathMoveBoat(self, nBoat, uBoatCapn)
  elseif State == 1 then
    local uHuang = Pg.GetGuidByName("RiverBoat_" .. nBoat)
    if uHuang then
      local uBoatCapn = Vehicle.GetDriver(uHuang)
      if uBoatCapn and not Object.IsPlayerControlled(Pg.GetGuidByName("RiverBoat_" .. nBoat)) then
        Ai.Anchor({AIGuid = uBoatCapn, AnchorRadius = 0})
        Debug.Printf(nBoat .. " Made it to the end of the path OR Attacking!!")
        local uFire1 = Pg.GetGuidByName("loc_FireTgt_Boat" .. nBoat .. "_1")
        Ai.Goal({
          AIGuid = uBoatCapn,
          Goal = "Attack",
          Force = true,
          Target = uFire1,
          Priority = "hiPri"
        })
        self:_CreateEvent(Event.TimerRelative, {18}, RiverAttackStop, {
          self,
          nBoat,
          uBoatCapn
        })
        eBoatDamage = self:_CreateEvent(Event.TimerRelative, {4}, DamageCaracas, {self})
      end
    end
  end
end

function RiverAttackStop(self, nBoat, uBoatCapn)
  local uHuang = Pg.GetGuidByName("RiverBoat_" .. nBoat)
  if uHuang then
    local uBoatCapn = Vehicle.GetDriver(uHuang)
    if uBoatCapn and not Object.IsPlayerControlled(Pg.GetGuidByName("RiverBoat_" .. nBoat)) then
      Ai.Goal({
        AIGuid = uBoatCapn,
        Force = true,
        Goal = "Idle",
        LeaveTurretOn = true,
        Priority = "hiPri"
      })
      self:_CreateEvent(Event.TimerRelative, {15}, RiverAttack, {
        self,
        nBoat,
        uBoatCapn,
        1
      })
    end
  end
end

function ShellCaracas(self, sLauncher)
  DamageCaracas(self)
  local sPopLoc = "loc_strike_" .. sLauncher .. "_2"
  self:_CreateEvent(Event.TimerRelative, {0.4}, MPpop, {self, sPopLoc})
  local sPopLoc = "loc_strike_" .. sLauncher .. "_3"
  self:_CreateEvent(Event.TimerRelative, {0.8}, MPpop, {self, sPopLoc})
  local sPopLoc = "loc_strike_" .. sLauncher .. "_4"
  self:_CreateEvent(Event.TimerRelative, {1.4}, MPpop, {self, sPopLoc})
  local sPopLoc = "loc_strike_" .. sLauncher .. "_5"
  self:_CreateEvent(Event.TimerRelative, {1.7}, MPpop, {self, sPopLoc})
  local sPopLoc = "loc_strike_" .. sLauncher .. "_6"
  self:_CreateEvent(Event.TimerRelative, {2}, MPpop, {self, sPopLoc})
  local sPopLoc = "loc_strike_" .. sLauncher .. "_7"
  self:_CreateEvent(Event.TimerRelative, {2.6}, MPpop, {self, sPopLoc})
  local sPopLoc = "loc_strike_" .. sLauncher .. "_8"
  self:_CreateEvent(Event.TimerRelative, {3}, MPpop, {self, sPopLoc})
  local sPopLoc = "loc_strike_" .. sLauncher .. "_9"
  self:_CreateEvent(Event.TimerRelative, {3.4}, MPpop, {self, sPopLoc})
  local sPopLoc = "loc_strike_" .. sLauncher .. "_10"
  self:_CreateEvent(Event.TimerRelative, {3.8}, MPpop, {self, sPopLoc})
  local sPopLoc = "loc_strike_" .. sLauncher .. "_11"
  self:_CreateEvent(Event.TimerRelative, {4}, MPpop, {self, sPopLoc})
end

function MPpop(self, sPopLoc)
  local uPopper = Pg.GetGuidByName(sPopLoc)
  if Object.IsAwake(uPopper) then
    uPopX, uPopY, uPopZ = Object.GetPosition(uPopper)
    Pop(uPopX, uPopY, uPopZ, uPopper)
    if Net.IsActive() then
      Net.SendCustomEvent("AllCon002", NETEVENT_STARTEMITTERS, {
        uPopX,
        uPopY,
        uPopZ,
        uPopper
      })
    end
  end
end

function Pop(uPopX, uPopY, uPopZ, uPopper)
  if Object.IsAwake(uPopper) then
    Pg.Spawn("global_particle_airstrike_distance", uPopX, uPopY, uPopZ)
  end
end

function DisplayCaracasHealth(self)
  Hud.ObjectiveTray:SetSlotToText({
    nSlot = 1,
    sText = "[red][AllCon002.Objectives.caracasHealth]"
  })
  eDisplay1 = self:_CreateEvent(Event.TimerRelative, {0.5}, function()
    Hud.ObjectiveTray:SetSlotToText({
      nSlot = 1,
      sText = "[white][AllCon002.Objectives.caracasHealth]"
    })
  end)
  eDisplay2 = self:_CreateEvent(Event.TimerRelative, {1}, function()
    Hud.ObjectiveTray:SetSlotToText({
      nSlot = 1,
      sText = "[red][AllCon002.Objectives.caracasHealth]"
    })
  end)
  eDisplay3 = self:_CreateEvent(Event.TimerRelative, {1.5}, function()
    Hud.ObjectiveTray:SetSlotToText({
      nSlot = 1,
      sText = "[white][AllCon002.Objectives.caracasHealth]"
    })
  end)
  local sBarColor = "green"
  if nCaracasLife <= 60 then
    sBarColor = "yellow"
  end
  if nCaracasLife <= 25 then
    sBarColor = "red"
  end
  if 1 > nCaracasLife then
    nCaracasLife = 0
  end
  sHudText = "[" .. sBarColor .. "][bar" .. nCaracasLife .. "]"
  Hud.ObjectiveTray:SetSlotToText({
    vPlayer = nil,
    nSlot = 2,
    sText = sHudText
  })
end

function DamageCaracas(self)
  nCaracasLife = nCaracasLife - nDamMod
  Debug.Printf(nAAalive .. "######################  Caracas is now at" .. nCaracasLife)
  DisplayCaracasHealth(self)
  if nCaracasLife == 98 then
    MPplumes(1)
    MPplumes(2)
    MPplumes(12)
  elseif nCaracasLife == 90 then
    MrxVoSequence.Start({
      "Fiona-In-Mission-Contract-All02-18"
    })
  elseif nCaracasLife == 44 then
    MPplumes(3)
    MPplumes(4)
    MPplumes(11)
    MrxVoSequence.Start({
      "Fiona-In-Mission-Contract-All02-17"
    })
  elseif nCaracasLife == 24 then
    MPplumes(5)
    MPplumes(6)
    MPplumes(10)
    MrxVoSequence.Start({
      "Fiona-In-Mission-Contract-All02-19"
    })
  elseif nCaracasLife == 18 then
    MPplumes(7)
    MPplumes(8)
    MPplumes(9)
    MrxVoSequence.Start({
      "Fiona-In-Mission-Contract-All02-20"
    })
  end
  if nCaracasLife <= 3 then
    bBombard = 0
    self:_SetCancelMessage("[AllCon002.Terms.Cancel01]")
    MrxVoSequence.Start({
      "Fiona-In-Mission-Contract-All02-21"
    })
    self:_CreateEvent(Event.TimerRelative, {4.5}, self.Cancel, {self})
  end
end

function MPplumes(nPlumeID)
  local uPlume = Pg.GetGuidByName("Loc_Smoke_" .. nPlumeID)
  if Object.IsAwake(uPlume) then
    uPlumeX, uPlumeY, uPlumeZ = Object.GetPosition(uPlume)
    Plumes(uPlumeX, uPlumeY, uPlumeZ, nPlumeID)
    if Net.IsActive() then
      Net.SendCustomEvent("AllCon002", NETEVENT_STARTPLUMES, {
        uPlumeX,
        uPlumeY,
        uPlumeZ,
        nPlumeID
      })
    end
  else
    Debug.Printf("Noooooooooooooooooooooooooo, NO smoke locations awake?")
  end
end

function Plumes(uPlumeX, uPlumeY, uPlumeZ, nPlumeID)
  Debug.Printf("Plumes, Plumes in MP!!!!!!!!!!!!!!")
  if _tSmokeParticleObjects == nil then
    _tSmokeParticleObjects = {}
  end
  local uSmoke = Pg.Spawn("global_particle_env_smokeplume_distance_tall", uPlumeX, uPlumeY, uPlumeZ)
  table.insert(_tSmokeParticleObjects, uSmoke)
  nTotalPlumes = table.getn(_tSmokeParticleObjects)
end

function AirStriked(uStrikeLoc, uSeeStrike, uEncounter)
  local sAmmo = "Rocket Artillery Projectile"
  local nWidth = 5
  local nHeight = 4
  local nShells = 1
  local nTime = 15
  local uModeOfTrans = Player.GetControlledObject(Player.GetLocalPlayer())
  local uPlayerSpeed = Object.GetVelocity(uModeOfTrans) or 1
  if uPlayerSpeed < 10 then
    uFixedSpeed = (uPlayerSpeed + 4) * 10
    nStrikeFrequency = 6
  elseif uPlayerSpeed < 16 then
    nStrikeFrequency = 3
    uFixedSpeed = uPlayerSpeed * 11
  else
    uFixedSpeed = uPlayerSpeed * 11
    nStrikeFrequency = 1
    if uFixedSpeed > 250 then
      uFixedSpeed = 240
    end
  end
  Debug.Printf(uFixedSpeed .. "----------------------------------The player is moving at" .. uPlayerSpeed)
  local nGrenadeX, nGrenadeY, nGrenadeZ = Pg.FindPointFromCamera(uFixedSpeed, 20, -1)
  local nHeroX, nHeroY, nHeroZ = Object.GetPosition(Player.GetPrimaryCharacter())
  local nGrenadeX = nGrenadeX - (math.randf() * 7 - math.randf() * 7)
  local nHeightVectorX, nHeightVectorY, nHeightVectorZ = nGrenadeX - nHeroX, nGrenadeY - nHeroY, nGrenadeZ - nHeroZ
  nHeightVectorX, nHeightVectorY, nHeightVectorZ = Math.Normalize(nHeightVectorX, nHeightVectorY, nHeightVectorZ)
  local nWidthVectorX, nWidthVectorY, nWidthVectorZ = -(nGrenadeZ - nHeroZ), 0, nGrenadeX - nHeroX
  nWidthVectorX, nWidthVectorY, nWidthVectorZ = Math.Normalize(nWidthVectorX, nWidthVectorY, nWidthVectorZ)
  for i = 1, nShells do
    local nWidthIncrement = nWidth
    local nHeightIncrement = nHeight
    local nXAdjust = -(math.randf() * nWidthIncrement - math.randf() * nWidthIncrement + (2.5 - i) * nWidthIncrement)
    local nZAdjust = -(math.randf() * nHeightIncrement - math.randf() * nHeightIncrement)
    local tData = {}
    tData.sAmmo = sAmmo
    tData.nTargetX = nGrenadeX + nWidthVectorX * nXAdjust + nHeightVectorX * nZAdjust
    tData.nTargetY = nGrenadeY + 250
    tData.nTargetZ = nGrenadeZ + nWidthVectorZ * nXAdjust + nHeightVectorZ * nZAdjust
    if uStrikeLoc then
      local uLocGuid = Pg.GetGuidByName("loc_Rockets_" .. uEncounter .. "_" .. i)
      if uLocGuid then
        local nX, nY, nZ = Object.GetPosition(uLocGuid)
        local tData = {}
        tData.sAmmo = sAmmo
        tData.nTargetX = nX
        tData.nTargetY = nY + 250
        tData.nTargetZ = nZ
      else
        tData = nil
      end
    end
    local uPlayer = Pg.GetGuidByName("China")
    if tData then
      Event.Create(Event.TimerRelative, {
        2 + i * (nTime / 22)
      }, TriggerFallingMissile, {tData, uPlayer})
    end
  end
end

function MPShelling(uStrikeLoc, uSeeStrike, uEncounter)
  AirStriked(uStrikeLoc, uSeeStrike, uEncounter)
  if Net.IsActive() then
    Net.SendCustomEvent("AllCon002", NETEVENT_AIRSTRUCK, {
      uStrikeLoc,
      uSeeStrike,
      uEncounter
    })
  end
end

function TriggerFallingMissile(tData, uPlayer)
  local uOrdnanceGuid = Airstrike.SpawnOrdnance(tData.sAmmo, tData.nTargetX, tData.nTargetY, tData.nTargetZ, 0, -100, 0, "impact", 1, uPlayer)
end

function Flybys(self)
  tFlybys = {
    {
      {
        altitude = 90,
        speed = 120,
        template = "Support Vehicle (A10)",
        bMulti = true
      },
      {
        altitude = 60,
        speed = 120,
        template = "Support Vehicle (A10)",
        bMulti = true
      },
      {
        altitude = 100,
        speed = 200,
        template = "Support Vehicle (F35)",
        bMulti = true
      },
      {
        altitude = 180,
        speed = 220,
        template = "Support Vehicle (F117)"
      },
      {
        altitude = 250,
        speed = 120,
        template = "Support Vehicle (C130)"
      },
      {
        altitude = 180,
        speed = 120,
        template = "Support Vehicle (AC130)"
      },
      {
        altitude = 50,
        speed = 60,
        template = "Support Vehicle (Predator)"
      },
      {
        altitude = 60,
        speed = 60,
        template = "Support Vehicle (Predator)"
      }
    },
    {
      {
        altitude = 100,
        speed = 200,
        template = "Support Vehicle (Q5)",
        bMulti = true
      },
      {
        altitude = 200,
        speed = 200,
        template = "Support Vehicle (Q5)",
        bMulti = true
      },
      {
        altitude = 160,
        speed = 240,
        template = "Support Vehicle (Q5)",
        bMulti = true
      }
    }
  }
  local tFaction = tFlybys[Math.randi(table.getn(tFlybys))]
  local tData = tFaction[Math.randi(table.getn(tFaction))]
  local count = 1
  if tData.bMulti then
    count = Math.randi(3)
  end
  while 0 < count do
    local uChar = Player.GetLocalCharacter()
    if uChar then
      local tx, ty, tz = Pg.FindPointFromCamera(300, tData.altitude, 10, Player.GetLocalPlayer(), math.randi(360))
      local sx, sy, sz = Pg.FindPointFromCamera(300, tData.altitude, 10, Player.GetLocalPlayer(), math.randi(360))
      fx, fy, fz = tx, ty, tz
      if sx and sz and fy then
        fy = fy + Math.randi(15) - Math.randi(15)
        Airstrike.Flyby(tData.template, sx, sz, fx, fz, fy + tData.altitude, tData.speed)
      else
        Debug.Printf("Flyby error! sx = " .. tostring(sx) .. " sz = " .. tostring(sz) .. " fx = " .. tostring(fx) .. " fz = " .. tostring(fz) .. " fy = " .. tostring(fy))
      end
    else
      Debug.Printf("Flyby error! player character is nil ")
    end
    count = count - 1
  end
  self:_CreateEvent(Event.TimerRelative, {15}, Flybys, {self})
end

function SmokeClean()
  if _tSmokeParticleObjects then
    for i, uGuid in ipairs(_tSmokeParticleObjects) do
      Object.Remove(uGuid)
    end
  end
end

function Cleanup(self)
  MrxLayerManager.Add({
    "vz_state_staging_all_HQ"
  })
  local tAddLayers = {
    "vz_state_car_city_act1",
    "vz_state_car_shanty_act1"
  }
  MrxLayerManager.MarkForAddition(tAddLayers)
  if eBoatDamage then
    Event.Delete(eBoatDamage)
  end
  if eDamageCarac then
    Event.Delete(eDamageCarac)
  end
  if eDisplay1 then
    Event.Delete(eDisplay1)
  end
  if eDisplay2 then
    Event.Delete(eDisplay2)
  end
  if eDisplay3 then
    Event.Delete(eDisplay3)
  end
  Hud.ObjectiveTray:ClearSlot({vPlayer = nil, nSlot = 1})
  Hud.ObjectiveTray:ClearSlot({vPlayer = nil, nSlot = 2})
  self:_CreateEvent(Event.TimerRelative, {2.5}, function()
    Hud.ObjectiveTray:ClearSlot({vPlayer = nil, nSlot = 1})
  end)
  SmokeClean()
  if Net.IsActive() then
    Net.SendCustomEvent("AllCon002", NETEVENT_CLEANSMOKE, {})
  end
  MrxTaskContract.Cleanup(self)
end
