inherit("MrxTaskContract")
import("MrxPmc")
import("MrxSupportData")
import("MrxSubtitle")
import("MrxVoSequence")
import("MrxUtil")
import("MrxTimer")
import("MrxAchievements")
import("MrxState")
NETEVENT_SETSTARTUPWEAPONS = 0
NETEVENT_RETURNWEAPONS = 1
tLocalP2Weapons = nil
tP2Weapons = nil
bP2PresentAtStart = nil
P1BoundaryEvent = nil
P2BoundaryEvent = nil
evClientSetup = nil

function NetEventCallback(nEventId, tArgs)
  if nEventId == NETEVENT_SETSTARTUPWEAPONS then
    Debug.Printf("got NETEVENT_SETSTARTUPWEAPONS")
    if not tP2Weapons then
      if MrxState._GetTotalRefCount() == 0 then
        SetP2Weapons()
      else
        evClientSetup = Event.Create(Event.TimerRelative, {1}, NetEventCallback, {nEventId})
      end
    end
  elseif nEventId == NETEVENT_RETURNWEAPONS then
    Debug.Printf("got NETEVENT_RETURNWEAPONS")
    Event.Delete(evClientSetup)
    evClientSetup = nil
    if tP2Weapons then
      uCharacter = Player.GetSecondaryCharacter()
      for i, uWeapon in ipairs(tP2Weapons) do
        Debug.Printf("Printing tArgs" .. tostring(uWeapon))
      end
      Human.Inventory.SetAllWeapons(uCharacter, tP2Weapons)
      MrxUtil.EnableHeroWeapons(false)
      tP2Weapons = nil
      Player.SetAimMode(Player.GetPrimaryPlayer(), true)
      Player.SetAimMode(Player.GetSecondaryPlayer(), true)
    end
  end
end

function LoadAssets(self, tSaveData)
  Hud.SupportMenu:SetShootingGalleryMode({bEnable = true})
  tLayersToAdd = {
    "Vz_State_PmcCon018",
    "vz_State_PmcCon018_Veh"
  }
  MrxLayerManager.Add(tLayersToAdd, self.AssetsLoaded, {self})
  _SetupP1Weapons(self)
  if Player.GetSecondaryCharacter() then
    self:_CreateEvent(Event.ObjectHibernation, {
      Player.GetSecondaryCharacter(),
      "awake"
    }, function()
      Debug.Printf("attempting to give p2 correct weapons")
      Net.SendCustomEvent("PmcCon018", NETEVENT_SETSTARTUPWEAPONS, {}, true)
    end)
  end
end

function Activated(self)
  MrxTaskContract.Activated(self)
  MrxSupportData.AddFreebie("Practice Laser", 1, Player.GetPrimaryPlayer())
  if Player.GetSecondaryCharacter() then
    MrxSupportData.AddFreebie("Practice Laser", 1, Player.GetSecondaryPlayer())
    bP2PresentAtStart = true
  else
    MrxSupportData.AddFreebie("Practice Laser", 1, Player.GetPrimaryPlayer())
  end
  OCRelation = Ai.GetRelation(Pg.GetGuidByName("OC"), Pg.GetGuidByName("PMC"))
  Ai.SetRelation(Pg.GetGuidByName("OC"), Pg.GetGuidByName("PMC"), 100)
  StartingCash = nil
  oCancelEvent = nil
  oMoneyUpdate = nil
  oTimerEvent = nil
  oCurObjective = nil
  CurPoints = 0
  AirStrikes = 0
  nCompletions = self:GetNumCompletions()
  nDeleteIndex = 1
  tObjectsToDelete = {}
  if nCompletions >= 2 then
    PointGoal = 100
  elseif nCompletions == 1 then
    PointGoal = 80
  else
    PointGoal = 60
  end
  _evClientJoinedPMC018 = Event.CreatePersistent(Event.ScriptEvent, {
    "mpPlayerJoin",
    function(tData)
      return Net.IsServer() and not Player.IsLocal(tData[1])
    end
  }, Net.SendCustomEvent, {
    "PmcCon018",
    NETEVENT_SETSTARTUPWEAPONS,
    {},
    true
  })
  _ObjectiveStart(self)
  _RandomSpread(self)
  Player.SetAimMode(Player.GetPrimaryPlayer(), false)
  if Player.GetSecondaryPlayer() then
    Player.SetAimMode(Player.GetSecondaryPlayer(), false)
  end
  Hud.ObjectiveTray:SetSlotToText({
    nSlot = 1,
    sText = "[white]" .. "[PmcCon018.HUD.PointsEarned] " .. tostring(CurPoints)
  })
  Hud.ObjectiveTray:SetSlotToText({
    nSlot = 2,
    sText = "[white]" .. "[PmcCon018.HUD.PointsNeeded] " .. PointGoal
  })
  self:_CreateEvent(Event.ScriptEvent, {
    "Airstrike",
    function()
      return true
    end
  }, function()
    Debug.Printf("Airstrikes = 1")
    AirStrikeVO(self, CurPoints)
    AirStrikes = 1
    self:_CreateEvent(Event.ScriptEvent, {
      "Airstrike",
      function()
        return true
      end
    }, function()
      Debug.Printf("Airstrikes = 2")
      AirStrikeVO(self, CurPoints)
      AirStrikes = 2
      if oTimerEvent then
        Event.Delete(oTimerEvent)
      end
      oTimerEvent = self:_CreateEvent(Event.TimerRelative, {15}, _TimeUp, {self})
    end)
  end)
end

function _RandomSpread(self)
  tMarkersVehicles = {
    "PMCCon018_Obj_marker 0x0012d13c",
    "PMCCon018_Obj_marker 0x0012d13d",
    "PMCCon018_Obj_marker 0x0012d13f",
    "PMCCon018_Obj_marker 0x0012d140",
    "PMCCon018_Obj_marker 0x0012d141",
    "PMCCon018_Obj_marker 0x0012d143",
    "PMCCon018_Obj_marker 0x0012d145",
    "PMCCon018_Obj_marker 0x0012d146",
    "PMCCon018_Obj_marker 0x0012d152",
    "PMCCon018_Obj_marker 0x0012d153",
    "PMCCon018_Obj_marker 0x0012d154",
    "PMCCon018_Obj_marker 0x0012d156",
    "PMCCon018_Obj_marker 0x0012d157",
    "PMCCon018_Obj_marker 0x0012d159",
    "PMCCon018_Obj_marker 0x0012d15b",
    "PMCCon018_Obj_marker 0x0012d15d",
    "PMCCon018_Obj_marker 0x0012d15f",
    "PMCCon018_Obj_marker 0x0012d160",
    "PMCCon018_Obj_marker 0x0012d164",
    "PMCCon018_Obj_marker 0x0012d16c",
    "PMCCon018_Obj_marker 0x0012d1a2",
    "PMCCon018_Obj_marker 0x0012d1a3",
    "PMCCon018_Obj_marker 0x0012d1a4",
    "PMCCon018_Obj_marker 0x0012d1a5",
    "PMCCon018_Obj_marker 0x0012d1a8",
    "PMCCon018_Obj_marker 0x0012d1aa",
    "PMCCon018_Obj_marker 0x0012d1af",
    "PMCCon018_Obj_marker 0x0012d1b3",
    "PMCCon018_Obj_marker 0x0012d1b4",
    "PMCCon018_Obj_marker 0x0012d1b5",
    "PMCCon018_Obj_marker 0x0012d1b6",
    "PMCCon018_Obj_marker 0x0012d1ba",
    "PMCCon018_Obj_marker 0x0012d1ac"
  }
  tMarkersBarrels = {
    "PMCCon018_Obj_marker",
    "PMCCon018_Obj_marker 0x0012d13e",
    "PMCCon018_Obj_marker 0x0012d142",
    "PMCCon018_Obj_marker 0x0012d144",
    "PMCCon018_Obj_marker 0x0012d147",
    "PMCCon018_Obj_marker 0x0012d148",
    "PMCCon018_Obj_marker 0x0012d149",
    "PMCCon018_Obj_marker 0x0012d14a",
    "PMCCon018_Obj_marker 0x0012d14b",
    "PMCCon018_Obj_marker 0x0012d14c",
    "PMCCon018_Obj_marker 0x0012d14d",
    "PMCCon018_Obj_marker 0x0012d14e",
    "PMCCon018_Obj_marker 0x0012d14f",
    "PMCCon018_Obj_marker 0x0012d150",
    "PMCCon018_Obj_marker 0x0012d151",
    "PMCCon018_Obj_marker 0x0012d155",
    "PMCCon018_Obj_marker 0x0012d158",
    "PMCCon018_Obj_marker 0x0012d15a",
    "PMCCon018_Obj_marker 0x0012d15c",
    "PMCCon018_Obj_marker 0x0012d15e",
    "PMCCon018_Obj_marker 0x0012d161",
    "PMCCon018_Obj_marker 0x0012d162",
    "PMCCon018_Obj_marker 0x0012d163",
    "PMCCon018_Obj_marker 0x0012d165",
    "PMCCon018_Obj_marker 0x0012d166",
    "PMCCon018_Obj_marker 0x0012d167",
    "PMCCon018_Obj_marker 0x0012d168",
    "PMCCon018_Obj_marker 0x0012d169",
    "PMCCon018_Obj_marker 0x0012d16a",
    "PMCCon018_Obj_marker 0x0012d16b",
    "PMCCon018_Obj_marker 0x0012d16d",
    "PMCCon018_Obj_marker 0x0012d16e",
    "PMCCon018_Obj_marker 0x0012d1a6",
    "PMCCon018_Obj_marker 0x0012d1a7",
    "PMCCon018_Obj_marker 0x0012d1a9",
    "PMCCon018_Obj_marker 0x0012d1ab",
    "PMCCon018_Obj_marker 0x0012d1ad",
    "PMCCon018_Obj_marker 0x0012d1ae",
    "PMCCon018_Obj_marker 0x0012d1b1",
    "PMCCon018_Obj_marker 0x0012d1b2",
    "PMCCon018_Obj_marker 0x0012d1b7",
    "PMCCon018_Obj_marker 0x0012d1b8",
    "PMCCon018_Obj_marker 0x0012d1b9",
    "PMCCon018_Obj_marker 0x0012d1bb",
    "PMCCon018_Obj_marker 0x0012d1bc"
  }
  for i = 1, 8 do
    local index = GetRandomTableIndex(tMarkersVehicles)
    if Pg.GetGuidByName(tMarkersVehicles[index]) == nil then
      i = i - 1
    else
      local x, y, z = Object.GetPosition(Pg.GetGuidByName(tMarkersVehicles[index]))
      uSPAWN = Pg.Spawn("M35 (Cargo) (VZ)", x, y, z, 0, false, true)
      Object.SetTransformToObject(uSPAWN, Pg.GetGuidByName(tMarkersVehicles[index]))
      table.remove(tMarkersVehicles, index)
      self:_CreateEvent(Event.ObjectDeath, {uSPAWN}, _AddPoints, {3, self})
      tObjectsToDelete[nDeleteIndex] = uSPAWN
      nDeleteIndex = nDeleteIndex + 1
    end
  end
  for i = 1, 7 do
    local index = GetRandomTableIndex(tMarkersVehicles)
    if Pg.GetGuidByName(tMarkersVehicles[index]) == nil then
      i = i - 1
    else
      local x, y, z = Object.GetPosition(Pg.GetGuidByName(tMarkersVehicles[index]))
      uSPAWN = Pg.Spawn("_vzoutpost_fueltanks_PmcCon018", x, y, z, 0, false, true)
      Object.SetTransformToObject(uSPAWN, Pg.GetGuidByName(tMarkersVehicles[index]))
      table.remove(tMarkersVehicles, index)
      self:_CreateEvent(Event.ObjectDeath, {uSPAWN}, _AddPoints, {5, self})
      tObjectsToDelete[nDeleteIndex] = uSPAWN
      nDeleteIndex = nDeleteIndex + 1
    end
  end
  for i = 1, 9 do
    local index = GetRandomTableIndex(tMarkersVehicles)
    if Pg.GetGuidByName(tMarkersVehicles[index]) == nil then
      i = i - 1
    else
      local x, y, z = Object.GetPosition(Pg.GetGuidByName(tMarkersVehicles[index]))
      uSPAWN = Pg.Spawn("M151 .50Cal (VZ)", x, y, z, 0, false, true)
      Object.SetTransformToObject(uSPAWN, Pg.GetGuidByName(tMarkersVehicles[index]))
      table.remove(tMarkersVehicles, index)
      self:_CreateEvent(Event.ObjectDeath, {uSPAWN}, _AddPoints, {2, self})
      tObjectsToDelete[nDeleteIndex] = uSPAWN
      nDeleteIndex = nDeleteIndex + 1
    end
  end
  nTempCounter = 0
  for i, sMarkerName in ipairs(tMarkersVehicles) do
    if Pg.GetGuidByName(tMarkersVehicles[i]) then
      CoinToss = Math.randi(0, 3)
      Debug.Printf(tostring(CoinToss))
      if CoinToss == 0 or CoinToss == 1 or CoinToss == 3 then
        local x, y, z = Object.GetPosition(Pg.GetGuidByName(sMarkerName))
        uSPAWN = Pg.Spawn("_global_explosivebarrel_Long_Hibernation", x, y, z, 0, false, true)
        tObjectsToDelete[nDeleteIndex] = uSPAWN
        nDeleteIndex = nDeleteIndex + 1
        nTempCounter = nTempCounter + 1
        self:_CreateEvent(Event.ObjectDeath, {uSPAWN}, _AddPoints, {1, self})
      end
    end
  end
  for i, sMarkerName in ipairs(tMarkersBarrels) do
    if Pg.GetGuidByName(tMarkersBarrels[i]) then
      CoinToss = Math.randi(0, 3)
      Debug.Printf(tostring(CoinToss))
      if CoinToss == 0 or CoinToss == 1 or CoinToss == 3 then
        local x, y, z = Object.GetPosition(Pg.GetGuidByName(sMarkerName))
        uSPAWN = Pg.Spawn("_global_explosivebarrel_Long_Hibernation", x, y, z, 0, false, true)
        tObjectsToDelete[nDeleteIndex] = uSPAWN
        nDeleteIndex = nDeleteIndex + 1
        nTempCounter = nTempCounter + 1
        self:_CreateEvent(Event.ObjectDeath, {uSPAWN}, _AddPoints, {1, self})
      end
    end
  end
end

function _AddPoints(iPoints, self)
  if oTimerEvent then
    Event.Delete(oTimerEvent)
  end
  oTimerEvent = self:_CreateEvent(Event.TimerRelative, {8}, _TimeUp, {self})
  Debug.Printf("Just set up all the timers for a jeep or something")
  CurPoints = CurPoints + iPoints
  Hud.ObjectiveTray:SetSlotToText({
    nSlot = 1,
    sText = "[green]" .. "[PmcCon018.HUD.PointsEarned] " .. tostring(CurPoints)
  })
  self:_CreateEvent(Event.TimerRelative, {0.5}, function()
    Hud.ObjectiveTray:SetSlotToText({
      nSlot = 1,
      sText = "[white]" .. "[PmcCon018.HUD.PointsEarned] " .. tostring(CurPoints)
    })
  end)
end

function GetRandomTableIndex(t)
  local n = table.getn(t)
  if 0 < n then
    i = Math.randi(1, n)
  end
  return i
end

function _ObjectiveStart(self)
  if Player.GetSecondaryCharacter() then
    tLocalP2Weapons = Human.Inventory.GetAllWeapons(Player.GetSecondaryCharacter())
  end
  local nCompletions = self:GetNumCompletions()
  if nCompletions == 0 then
    sDescription = "[PmcCon018.objectives.001]"
  elseif nCompletions == 1 then
    sDescription = "[PmcCon018.objectives.002]"
  else
    sDescription = "[PmcCon018.objectives.003]"
  end
  oCurObjective = self:CreateChild({
    sName = "Burnout",
    sModuleName = "MrxTaskObjectiveDestroy",
    sDspShortDesc = sDescription,
    vTgtInclude = "PMCCon018_Obj_marker",
    bDspBlpWld = false,
    bDspBlpRdr = true,
    bDspBlpPda = true,
    fOnCancel = function()
      self.Cancel(self)
    end
  })
  self:_CreateEvent(Event.TimerRelative, {10}, function()
    P1BoundaryEvent = self:_CreateEvent(Event.Boundary, {
      Player.GetAnyCharacter(),
      Pg.GetGuidByName("PMCCon018OutOfBounds"),
      "exit"
    }, function()
      Debug.Printf("Triggered p1 OOB")
      MrxVoSequence.Start({
        "Misha-In-Mission-MinorContract-Pmc18-05",
        {
          _OutOfBounds,
          {self}
        }
      })
    end)
  end)
end

function _TimeUp(self)
  Debug.Printf("Airstrikes used = " .. AirStrikes)
  if AirStrikes == 2 then
    if CurPoints >= PointGoal then
      tPossibleVO = {
        "Misha-In-Mission-MinorContract-Pmc18-06",
        "Misha-In-Mission-MinorContract-Pmc18-07",
        "Misha-In-Mission-MinorContract-Pmc18-08"
      }
      sVOLine = MrxUtil.GetRandomTableElement(tPossibleVO)
      MrxVoSequence.Start({
        sVOLine,
        {
          self.Complete,
          {self}
        }
      })
    else
      if CurPoints < PointGoal then
        self:_SetCancelMessage("[PmcCon018.Terms.CancelPoints]")
        self.Cancel(self)
      else
      end
    end
  end
end

function _SetupP1Weapons(self)
  Debug.Printf("about inside _SetupP1Weapons")
  uCharacter = Player.GetLocalCharacter()
  tP1Weapons = Human.Inventory.GetAllWeapons(uCharacter)
  for i, uWeapon in ipairs(tP1Weapons) do
    Human.Inventory.DropWeapon(uCharacter, uWeapon)
    x, y, z = Object.GetPosition(uCharacter)
    Object.DisablePhysics(uWeapon)
    Object.SetPosition(uWeapon, x, y - 5, z)
  end
  Debug.Printf("about done with _SetupP1Weapons")
end

function SetP2Weapons()
  Debug.Printf("about inside SetP2Weapons")
  uCharacter = Player.GetSecondaryCharacter()
  tP2Weapons = Human.Inventory.GetAllWeapons(uCharacter)
  for i, uWeapon in ipairs(tP2Weapons) do
    Debug.Printf("Printing tArgs" .. tostring(uWeapon))
  end
  for i, uWeapon in ipairs(tP2Weapons) do
    Human.Inventory.DropWeapon(uCharacter, uWeapon)
    x, y, z = Object.GetPosition(uCharacter)
    Object.DisablePhysics(uWeapon)
    Object.SetPosition(uWeapon, x, y - 5, z)
  end
  Player.SetAimMode(Player.GetPrimaryPlayer(), false)
  Player.SetAimMode(Player.GetSecondaryPlayer(), false)
end

function OnPlayerJoined(self, iPlayerId, uPlayerGuid, uCharGuid)
  self:_CreateEvent(Event.TimerRelative, {10}, function()
    if Player.GetSecondaryCharacter() then
      tLocalP2Weapons = Human.Inventory.GetAllWeapons(Player.GetSecondaryCharacter())
    end
  end)
  Debug.Printf("deleting P1BoundaryEvent event")
  if P1BoundaryEvent then
    Event.Delete(P1BoundaryEvent)
    P1BoundaryEvent = nil
  end
  self:_CreateEvent(Event.TimerRelative, {10}, function()
    if P1BoundaryEvent then
      Event.Delete(P1BoundaryEvent)
      P1BoundaryEvent = nil
    end
    P1BoundaryEvent = self:_CreateEvent(Event.Boundary, {
      Player.GetAnyCharacter(),
      Pg.GetGuidByName("PMCCon018OutOfBounds"),
      "exit"
    }, function()
      MrxVoSequence.Start({
        "Misha-In-Mission-MinorContract-Pmc18-05",
        {
          _OutOfBounds,
          {self}
        }
      })
    end)
  end)
end

function OnPlayerLeft(self, iPlayerId, sPlayerName, bLocalPlayer)
  nP1Freebies = MrxPmc.GetFreebieQty("[support.airstrike.laserguidedbomb.name]")
  if bP2PresentAtStart and nP1Freebies + AirStrikes == 1 then
    Debug.Printf("Giving p1 the \"missing\" airstrike")
    MrxSupportData.AddFreebie("Practice Laser")
  end
end

function Cleanup(self)
  Event.Delete(_evClientJoinedPMC018)
  Hud.ObjectiveTray:ClearSlot({vPlayer = nil, nSlot = 1})
  Hud.ObjectiveTray:ClearSlot({vPlayer = nil, nSlot = 2})
  Hud.ObjectiveTray:ClearSlot({vPlayer = nil, nSlot = 3})
  Hud.SupportMenu:SetShootingGalleryMode({bEnable = false})
  Event.Delete(oCancelEvent)
  Event.Delete(oMoneyUpdate)
  if OOBTimer then
    OOBTimer:Stop()
  end
  MrxSupportData.RemoveFreebie("Practice Laser")
  Debug.Printf("Deleting tObjectsToDelete")
  for i, uObject in ipairs(tObjectsToDelete) do
    if uObject == nil then
      Debug.Printf("Guid for deletion was nil")
    else
      Debug.Printf("Deleting object " .. tostring(uObject))
      Object.Remove(uObject)
    end
  end
  Debug.Printf("Marking layers for removal")
  MrxLayerManager.MarkForRemoval({
    "vz_State_PmcCon018_Veh",
    "vz_State_PmcCon018"
  })
  Player.SetAimMode(Player.GetPrimaryPlayer(), true)
  if Player.GetSecondaryPlayer() then
    Player.SetAimMode(Player.GetSecondaryPlayer(), true)
  end
  Ai.SetRelation(Pg.GetGuidByName("OC"), Pg.GetGuidByName("PMC"), OCRelation)
  MrxTaskContract.Cleanup(self)
end

function _OutOfBounds(self)
  Debug.Printf("Inside OOB")
  self:_SetCancelMessage("[PmcCon018.Terms.CancelOOB]")
  if not OOBTimer then
    OOBTimer = MrxTimer:Create({
      nStartTime = 10,
      nStopTime = 0,
      nWarning = 5,
      iTray = 3,
      tDoneCallbacks = {
        {
          Cancel,
          {self}
        }
      }
    })
  end
  Debug.Printf("Inside OOB just before timer start")
  OOBTimer:Start()
  TurnOffTimer(self)
end

function TurnOffTimer(self)
  self:_CreateEvent(Event.Boundary, {
    Player.GetAllCharacters(),
    Pg.GetGuidByName("PMCCon018OutOfBounds"),
    "enter"
  }, function()
    if Player.GetSecondaryCharacter() then
      if Object.InsideBoundary(Player.GetPrimaryCharacter(), Pg.GetGuidByName("PMCCon018OutOfBounds")) == true and Object.InsideBoundary(Player.GetSecondaryCharacter(), Pg.GetGuidByName("PMCCon018OutOfBounds")) == true then
        OOBTimer:Stop()
        RestartOOB(self)
      else
        TurnOffTimer(self)
      end
    elseif Object.InsideBoundary(Player.GetPrimaryCharacter(), Pg.GetGuidByName("PMCCon018OutOfBounds")) == true then
      OOBTimer:Stop()
      RestartOOB(self)
    else
      TurnOffTimer(self)
    end
  end)
end

function RestartOOB(self)
  P1BoundaryEvent = self:_CreateEvent(Event.Boundary, {
    Player.GetAnyCharacter(),
    Pg.GetGuidByName("PMCCon018OutOfBounds"),
    "exit"
  }, function()
    if P1BoundaryEvent then
      Event.Delete(P1BoundaryEvent)
    end
    if P2BoundaryEvent then
      Event.Delete(P2BoundaryEvent)
    end
    MrxVoSequence.Start({
      "Misha-In-Mission-MinorContract-Pmc18-05",
      {
        _OutOfBounds,
        {self}
      }
    })
  end)
end

function AirStrikeVO(self, InitialPoints)
  self:_CreateEvent(Event.TimerRelative, {10}, function()
    if InitialPoints == CurPoints then
      tChrisNegativeVO = {
        "Chris.BadNews03",
        "Chris.Misc.Negative01",
        "Chris.Misc.Negative02",
        "Chris.Misc.Negative03",
        "Chris.Misc.Negative04",
        "Chris.Misc.Negative05"
      }
      tMattiasNegativeVO = {
        "Mattias.BadNews01",
        "Mattias.Misc.Negative05",
        "Mattias.Misc.Negative01",
        "Mattias.Misc.Negative04",
        "Mattias.Misc.Negative01",
        "Mattias.Misc.Negative02",
        "Mattias.Misc.Negative03",
        "Mattias.Misc.Negative04",
        "Mattias.Misc.Negative05",
        "Mattias.BadNews03"
      }
      tJenNegativeVO = {
        "Jen.Negative01",
        "Jen.Negative02",
        "Jen.Negative04",
        "Jen.Negative05",
        "Jen.BadNews01"
      }
      MrxVoSequence.Start({
        {
          mattias = MrxUtil.GetRandomTableElement(tMattiasNegativeVO),
          jennifer = MrxUtil.GetRandomTableElement(tJenNegativeVO),
          chris = MrxUtil.GetRandomTableElement(tChrisNegativeVO)
        }
      })
    end
  end)
end

function Complete(self)
  uCharacter = Player.GetLocalCharacter()
  Net.SendCustomEvent("PmcCon018", NETEVENT_RETURNWEAPONS, {}, true)
  Human.Inventory.SetAllWeapons(uCharacter, tP1Weapons)
  Human.DisableWeapons(uCharacter)
  if Player.GetSecondaryCharacter() then
    Human.DisableWeapons(Player.GetSecondaryCharacter())
  end
  MrxAchievements.NetGrantAchievement("ACHIEVEMENT_GONE_SHOOTIN")
  MrxTaskContract.Complete(self)
end

function Cancel(self)
  uCharacter = Player.GetLocalCharacter()
  Net.SendCustomEvent("PmcCon018", NETEVENT_RETURNWEAPONS, {}, true)
  Human.Inventory.SetAllWeapons(uCharacter, tP1Weapons)
  Human.DisableWeapons(uCharacter)
  if Player.GetSecondaryCharacter() then
    Human.DisableWeapons(Player.GetSecondaryCharacter())
  end
  MrxTaskContract.Cancel(self)
end
