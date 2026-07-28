import("MrxSubtitle")
inherit("MrxTaskContract")
import("MrxVoSequence")
import("DangerousBuilding")
NETEVENT_CLIENTSETUP = 0

function Activated(self)
  bEarlyFind = false
  bTalked = false
  nAlreadyHeard = 0
  MrxTaskContract.Activated(self)
  Pg.LoadAsset("all_aisoldiermale_bare_upright_taunt_fb", "animation")
  DangerousBuilding.SetRarity("all", "never")
  local uPsych = Pg.GetGuidByName("psych")
  Vehicle.Usable(uPsych, false)
  self:_CreateEvent(Event.TimerRelative, {2}, KillTrucks, {self})
  tStartTalk = {
    {
      mattias = "Mattias-In-Mission-Contract-Pmc01-163",
      jennifer = "Jennifer-In-Mission-Contract-Pmc01-164",
      chris = "Chris-In-Mission-Contract-Pmc01-165"
    },
    "Fiona-In-Mission-Contract-Pmc01-166",
    {
      mattias = "Mattias-In-Mission-Contract-Pmc01-172",
      jennifer = "Jennifer-In-Mission-Contract-Pmc01-170",
      chris = "Chris-In-Mission-Contract-Pmc01-171"
    },
    "Fiona-In-Mission-Contract-Oil020-22",
    "Fiona-In-Mission-Contract-Oil020-24",
    "Fiona-In-Mission-Contract-Oil020-23"
  }
  oTalk = self:CreateChild({
    sName = "OilCon021: Get the Devastator",
    sModuleName = "MrxTaskObjectiveAction",
    sActionLabel = "[ContextAction.Talk]",
    vVoSeqOnAdd = tStartTalk,
    vTgtInclude = "MailTalk",
    sDspShortDesc = "[OilCon021.Objectives.001]",
    tOnPartComplete = {
      {
        SetupDeliverTruck,
        {self}
      }
    },
    fOnCancel = function()
      self:_SetCancelMessage("[GurCon003.Terms.Cancel03]")
      self.Cancel(self)
    end
  })
  eDevasDestroyed = self:_CreateEvent(Event.ObjectDeath, {
    Pg.GetGuidByName("MailTruck")
  }, TruckLost, {self})
  Event.Create(Event.ObjectPhysicsEvent, {
    Pg.GetGuidByName("MailTruck"),
    "VehicleSinking"
  }, TruckLost, {self})
  self:_CreateEvent(Event.ObjectProximity, {
    Player.GetAnyCharacter(),
    Pg.GetGuidByName("MailTalk"),
    "<",
    60,
    false,
    false
  }, SetupContactGuy, {self})
  self:_CreateEvent(Event.ObjectProximity, {
    Player.GetAnyCharacter(),
    Pg.GetGuidByName("MailTruck"),
    "<",
    4,
    false,
    false
  }, SpottedDevastator, {self})
  self:_CreateEvent(Event.ObjectDeath, {
    Pg.GetGuidByName("intercom_locked")
  }, DropDestroyed, {self})
  Net.SendCustomEvent("OilCon021", NETEVENT_CLIENTSETUP, {})
end

function DropDestroyed(self)
  self:_SetCancelMessage("[OilCon020.Terms.Cancel03]")
  self:Cancel()
end

function CheckForHostile(self)
  eHostileCheck = self:_CreateEvent(Event.TimerRelative, {1}, CheckForHostile, {self})
  local uHostile = Ai.GetFeeling(Pg.GetGuidByName("MailTalk"), Player.GetLocalCharacter())
  if uHostile and uHostile <= -33 then
    Event.Delete(eHostileCheck)
    Ai.Role({
      AIGuid = Pg.GetGuidByName("MailTalk"),
      Role = "Idle",
      Priority = "loPri"
    })
    bEarlyFind = true
    self:_SetCancelMessage("[OilCon021.Terms.Cancel02]")
    MrxVoSequence.Start({
      "Fiona.fio_g73",
      {
        self.Cancel,
        {self}
      }
    })
  end
end

function SetupContactGuy(self)
  CheckForHostile(self)
  Ai.SetInfractionMultiplier(GetGuidByName("OC"), 0.1)
  local uSitter = {
    AIGuid = Pg.GetGuidByName("MailTalk"),
    Target = Pg.GetGuidByName("UPseat"),
    Goal = "Enter",
    Priority = "hiPri",
    Callback = Seated,
    CallbackData = {self}
  }
  self:_CreateEvent(Event.TimerRelative, {2}, Ai.Goal, {uSitter})
end

function Seated(self)
  Ai.Goal({
    AIGuid = Pg.GetGuidByName("MailTalk"),
    Goal = "Idle",
    Priority = "hiPri"
  })
  self:_CreateEvent(Event.ObjectProximity, {
    Player.GetAnyCharacter(),
    Pg.GetGuidByName("MailTalk"),
    "<",
    4,
    false,
    false
  }, Conversation, {self})
end

function Conversation(self, tPlayerTalker)
  Ai.Goal({
    AIGuid = Pg.GetGuidByName("MailTalk"),
    Goal = "Exit",
    Priority = "hiPri",
    Force = true,
    Callback = KeepFacing,
    CallbackData = {self, tPlayerTalker}
  })
end

function KeepFacing(self, tPlayerTalker)
  Ai.Goal({
    AIGuid = Pg.GetGuidByName("MailTalk"),
    Target = tPlayerTalker[1],
    Goal = "Face",
    Position = true,
    Priority = "hiPri"
  })
  eFacing = self:_CreateEvent(Event.TimerRelative, {4}, KeepFacing, {self, tPlayerTalker})
  Ai.Role({
    AIGuid = Pg.GetGuidByName("MailTalk"),
    Role = "Idle",
    Priority = "hiPri"
  })
end

function Turn(self, uTalked)
  if eFacing then
    Event.Delete(eFacing)
  end
  uFacer = {
    AIGuid = Pg.GetGuidByName("MailTalk"),
    Target = uTalked,
    Goal = "Face",
    Position = true,
    Priority = "hiPri",
    Callback = Speak,
    CallbackData = {self}
  }
  self:_CreateEvent(Event.TimerRelative, {2}, Ai.Goal, {uFacer})
end

function Speak(self)
  Debug.Printf("Speak triggerered")
  uSpeak = Ai.Goal({
    AIGuid = Pg.GetGuidByName("MailTalk"),
    Target = Player.GetLocalCharacter(),
    Goal = "Speak",
    Priority = "hiPri"
  })
end

function TruckLost(self)
  self:_SetCancelMessage("[OilCon021.Terms.Cancel01]")
  MrxVoSequence.Start({
    "Fiona-In-Mission-Contract-Oil021-09",
    {
      self.Cancel,
      {self}
    }
  })
end

function SetupDeliverTruck(self, uTalked)
  bTalked = true
  if bEarlyFind == false then
    Event.Delete(eEarlyEnter)
    Turn(self, uTalked)
    if Object.HasLabel(uTalked, "Mattias") then
      sTalkedVO = "Mattias-In-Mission-Contract-Oil021-01"
    elseif Object.HasLabel(uTalked, "Jennifer") then
      sTalkedVO = "Jennifer-In-Mission-Contract-Oil021-02"
    elseif Object.HasLabel(uTalked, "Chris") then
      sTalkedVO = "Chris-In-Mission-Contract-Oil021-03"
    end
    MrxVoSequence.Start({
      {sTalkedVO, uTalked},
      {
        "OCMerc-In-Mission-Contract-Oil021-04",
        Pg.GetGuidByName("MailTalk")
      },
      {
        Human.DoAction,
        {
          Pg.GetGuidByName("MailTalk"),
          "ExitAction"
        }
      },
      "Fiona-In-Mission-Contract-Oil020-25"
    })
    local uSoldier = Pg.GetGuidByName("MailTalk")
    if uSoldier then
      self:_CreateEvent(Event.TimerRelative, {3.5}, Human.PlayRawAnimation, {
        uSoldier,
        "all_aisoldiermale_bare_upright_taunt_fb",
        false,
        false,
        0,
        false
      })
    end
  end
  DeliverTruckObjective(self)
end

function EarlyFind(self)
  bEarlyFind = true
  oTalk:Configure({bDsp = false})
  oTalk:Complete()
  DeliverTruckObjective(self)
end

function DeliverTruckObjective(self)
  Ai.Role({
    AIGuid = Pg.GetGuidByName("MailTalk"),
    Role = "Idle",
    Priority = "loPri"
  })
  Event.Delete(eHostileCheck)
  Event.Delete(eDevasDestroyed)
  self:_CreateEvent(Event.ObjectInSeat, {
    Player.GetAnyCharacter(),
    Pg.GetGuidByName("MailTruck"),
    "a",
    "e"
  }, MrxMusic.PlaySpecialMusic, {
    "mu_mission_oilcon021_01"
  })
  oDevast = self:CreateChild({
    sName = "OilCon021: Deliver the Devastator",
    sModuleName = "MrxTaskObjectiveDeliver",
    vTgtInclude = "MailTruck",
    vDestLoc = Pg.GetGuidByName("loc_MailDrop"),
    fDist = 8,
    bStop = true,
    bXZOnly = true,
    sDspShortDesc = "[OilCon021.Objectives.002]",
    fOnComplete = function()
      MrxVoSequence.Start({
        "Fiona-In-Mission-Contract-Oil021-12",
        {
          self.Complete,
          {self}
        }
      })
    end,
    fOnCancel = function()
      if not Object.IsAlive(Pg.GetGuidByName("MailTruck")) then
        self:_SetCancelMessage("[OilCon021.Terms.Cancel01]")
      end
      self:Cancel()
    end
  })
  self:_CreateEvent(Event.ObjectProximity, {
    Pg.GetGuidByName("MailTruck"),
    Pg.GetGuidByName("OilCon021_Laugher_4"),
    "<",
    20,
    false,
    false
  }, Mocking, {self})
  self:_CreateEvent(Event.ObjectInSeat, {
    Player.GetAnyCharacter(),
    Pg.GetGuidByName("psych"),
    "a",
    "e"
  }, FionaVOwrong, {self})
end

function SpottedDevastator(self, uNearDeva)
  if bTalked == false then
    EarlyFind(self)
  end
  Debug.Printf("Spotted truck started %%%%%%%%%%%%%%%%%%%%%%%%%%")
  if Object.HasLabel(uNearDeva[1], "Mattias") then
    sFoundVO = "Mattias-In-Mission-Contract-Pmc01-131"
    sFoundVO2 = "Mattias-In-Mission-Contract-Pmc01-141"
    sFoundVO3 = "Mattias-In-Mission-Contract-Pmc01-147"
  elseif Object.HasLabel(uNearDeva[1], "Jennifer") then
    sFoundVO = "Jennifer-In-Mission-Contract-Pmc01-132"
    sFoundVO2 = "Jennifer-In-Mission-Contract-Pmc01-142"
    sFoundVO3 = "Jennifer-In-Mission-Contract-Pmc01-145"
  elseif Object.HasLabel(uNearDeva[1], "Chris") then
    sFoundVO = "Chris-In-Mission-Contract-Pmc01-133"
    sFoundVO2 = "Chris-In-Mission-Contract-Pmc01-143"
    sFoundVO3 = "Chris-In-Mission-Contract-Pmc01-146"
  end
  MrxVoSequence.Start({
    {
      sFoundVO,
      uNearDeva[1]
    },
    "Fiona-In-Mission-MinorContract-Oil03-12",
    {
      sFoundVO2,
      uNearDeva[1]
    },
    "Fiona-In-Mission-Contract-Pmc01-144",
    {
      sFoundVO3,
      uNearDeva[1]
    },
    {3},
    {
      OndaflyMocking,
      {self}
    }
  })
  Debug.Printf("There it is!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!")
end

function OndaflyMocking(self)
  local x, y, z = Object.GetPosition(Pg.GetGuidByName("MailTruck"))
  local tOCmockers = {}
  local tOCmockers = Pg.FastCollectHumans(x, y, z, 10, "OC && Human")
  nOCmocker = table.getn(tOCmockers)
  if nOCmocker > 0 then
    DotheMocking(self, tOCmockers[1])
    Debug.Printf([[
MMMMMMMMMMMMMMMMMMMMMMMMMMMMMMMMMMMM 
  MMMMMMMMocking, mocking in progress]])
  else
    self:_CreateEvent(Event.TimerRelative, {3}, OndaflyMocking, {self})
  end
end

function DotheMocking(self, uMocker)
  local uClown = Vehicle.GetDriver(Pg.GetGuidByName("MailTruck"))
  if uClown then
    nAlreadyHeard = nAlreadyHeard + 1
    if nAlreadyHeard == 1 then
      self:_CreateEvent(Event.TimerRelative, {0.5}, function(self)
        if Object.HasLabel(uClown, "Mattias") then
          sMockVO1 = "Mattias-In-Mission-Contract-Pmc01-148"
        elseif Object.HasLabel(uClown, "Jennifer") then
          sMockVO1 = "Jennifer-In-Mission-Contract-Pmc01-149"
        elseif Object.HasLabel(uClown, "Chris") then
          sMockVO1 = "Chris-In-Mission-Contract-Pmc01-150"
        end
        MrxVoSequence.Start({
          {
            "OCMerc-In-Mission-Contract-Pmc01-161",
            uMocker
          },
          {sMockVO1, uClown},
          "Fiona-In-Mission-Contract-Pmc01-152"
        })
      end, {self})
    elseif nAlreadyHeard == 2 then
      self:_CreateEvent(Event.TimerRelative, {0.5}, function(self)
        if Object.HasLabel(uClown, "Mattias") then
          sMockVO2 = "Mattias-In-Mission-Contract-Pmc01-154"
        elseif Object.HasLabel(uClown, "Jennifer") then
          sMockVO2 = "Jennifer-In-Mission-Contract-Pmc01-155"
        elseif Object.HasLabel(uClown, "Chris") then
          sMockVO2 = "Chris-In-Mission-Contract-Pmc01-156"
        end
        MrxVoSequence.Start({
          {
            "OCMerc-In-Mission-Contract-Pmc01-158",
            uMocker
          },
          {sMockVO2, uClown},
          "Fiona-In-Mission-Contract-Pmc01-153",
          "Fiona-In-Mission-Contract-Pmc01-157"
        })
      end, {self})
    elseif nAlreadyHeard == 4 then
      self:_CreateEvent(Event.TimerRelative, {0.5}, function(self)
        if Object.HasLabel(uClown, "Mattias") then
          sMockVO4 = "Mattias-In-Mission-Contract-Oil021-06"
        elseif Object.HasLabel(uClown, "Jennifer") then
          sMockVO4 = "Jennifer-In-Mission-Contract-Oil021-07"
        elseif Object.HasLabel(uClown, "Chris") then
          sMockVO4 = "Chris-In-Mission-Contract-Oil021-08"
        end
        MrxVoSequence.Start({
          {
            "OCMerc-In-Mission-Contract-Pmc01-159",
            uMocker
          },
          {sMockVO4, uClown}
        })
      end, {self})
    elseif nAlreadyHeard == 6 then
      self:_CreateEvent(Event.TimerRelative, {0.5}, function(self)
        if Object.HasLabel(uClown, "Mattias") then
          sMockVO5 = "Mattias-In-Mission-Contract-Pmc01-167"
        elseif Object.HasLabel(uClown, "Jennifer") then
          sMockVO5 = "Jennifer-In-Mission-Contract-Pmc01-168"
        elseif Object.HasLabel(uClown, "Chris") then
          sMockVO5 = "Chris-In-Mission-Contract-Pmc01-169"
        end
        MrxVoSequence.Start({
          {
            "OCMerc-In-Mission-Contract-Pmc01-158",
            uMocker
          },
          {sMockVO5, uClown}
        })
      end, {self})
    else
      local tVo = {
        {
          "OCMerc-In-Mission-Contract-Pmc01-158",
          uMocker
        },
        {
          "OCMerc-In-Mission-Contract-Pmc01-159",
          uMocker
        },
        {
          "OCMerc-In-Mission-Contract-Pmc01-161",
          uMocker
        },
        {
          "OCMerc-In-Mission-Contract-Pmc01-162",
          uMocker
        },
        {
          "OCMerc-In-Mission-Contract-Pmc01-160",
          uMocker
        }
      }
      local sSelectedVo = MrxUtil.GetRandomTableElement(tVo)
      MrxVoSequence.Start({sSelectedVo})
    end
  end
  self:_CreateEvent(Event.TimerRelative, {12}, OndaflyMocking, {self})
end

function Mocking(self)
  for i = 1, 4 do
    local uLaughing = Pg.GetGuidByName("OilCon021_Laugher_" .. i)
    local uStartLaugh = i / 2
    if uLaughing then
      self:_CreateEvent(Event.TimerRelative, {uStartLaugh}, Human.PlayRawAnimation, {
        uLaughing,
        "all_aisoldiermale_bare_upright_laugh_fb",
        false,
        false,
        0,
        false
      })
    end
  end
  self:_CreateEvent(Event.TimerRelative, {4}, Human.PlayRawAnimation, {
    Pg.GetGuidByName("OilCon021_Laugher_3"),
    "all_aisoldiermale_bare_upright_laugh_fb",
    false,
    false,
    0,
    false
  })
end

function FionaVOwrong(self)
  MrxVoSequence.Start({
    "Fiona-In-Mission-Contract-Oil020-26"
  })
end

function FionaVOroad(self)
  MrxVoSequence.Start({
    "Fiona-In-Mission-Contract-Oil021-05",
    {
      mattias = "Mattias-In-Mission-Contract-Oil021-06",
      jennifer = "Jennifer-In-Mission-Contract-Oil021-07",
      chris = "Chris-In-Mission-Contract-Oil021-08"
    }
  })
end

function KillTrucks(self)
  uKilled = Pg.GetGuidByName("OilCon021_deadblocker")
  if uKilled then
    Object.Kill(uKilled)
  end
end

function OnPlayerJoined(self, iPlayerId, uPlayerGuid, uCharGuid)
  Net.SendCustomEvent("OilCon021", NETEVENT_CLIENTSETUP, {})
end

function NetEventCallback(nEventId, tArgs)
  if nEventId == NETEVENT_CLIENTSETUP then
  end
end

function Cleanup(self)
  if eEarlyEnter then
    Event.Delete(eEarlyEnter)
  end
  DangerousBuilding.SetRarity("all", "default")
  MrxTaskContract.Cleanup(self)
end
