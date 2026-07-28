function Init()
  tEvents = {}
  
  tSpeakers = {}
  tSpeechIndex = {}
  tSpeech = {
    "GuerillaSoldier_Rebecca01_Guerilla Soldier_Prop1",
    "GuerillaSoldier_Rebecca01_Guerilla Soldier_Prop2",
    "GuerillaSoldier_Rebecca01_Guerilla Soldier_Prop3",
    "GuerillaSoldier_Rebecca01_Guerilla Soldier_Prop4",
    "GuerillaSoldier_Rebecca01_Guerilla Soldier_Prop5"
  }
end

function Deinit()
  tEvents = nil
  tSpeakers = nil
  tSpeechIndex = nil
  tSpeech = nil
end

function OnActivate(uGuid, args)
  if Object.IsAlive(uGuid) then
    local myEvents = {}
    myEvents.eDeath = Event.Create(Event.ObjectDeath, {uGuid}, OnDeath)
    tEvents[uGuid] = myEvents
    local tRiders = Vehicle.GetRiders(uGuid)
    if 0 < #tRiders then
      OnEnter(tRiders[1], uGuid)
    end
  end
end

function OnDeactivate(uGuid, args)
  local myEvents = tEvents[uGuid]
  if type(myEvents) == "table" then
    for i, e in pairs(myEvents) do
      Event.Delete(e)
    end
  else
    Event.Delete(myEvents)
  end
  tEvents[uGuid] = nil
  tSpeakers[uGuid] = nil
  tSpeechIndex[uGuid] = nil
end

function OnExit(uRider, uGuid)
  local myEvents = tEvents[uGuid]
  myEvents.eEntryExit = Event.Create(Event.ObjectInSeat, {
    uRider,
    uGuid,
    "a",
    "e"
  }, OnEnter)
  tEvents[uGuid] = myEvents
  CancelCue(uGuid)
  tSpeakers[uGuid] = nil
end

function OnEnter(uRider, uGuid)
  local myEvents = tEvents[uGuid]
  myEvents.eEntryExit = Event.Create(Event.ObjectInSeat, {
    uRider,
    uGuid,
    "a",
    "x"
  }, OnExit)
  tEvents[uGuid] = myEvents
  tSpeakers[uGuid] = uRider
  tSpeechIndex[uGuid] = 1
  StartNextCue(uGuid)
end

function OnDeath(uGuid)
  CancelCue(uGuid)
  OnDeactivate(uGuid)
end

function CueFinished(uGuid)
  if tSpeechIndex and tSpeechIndex[uGuid] then
    tSpeechIndex[uGuid] = tSpeechIndex[uGuid] + 1
    if tSpeechIndex[uGuid] > 5 then
      tSpeechIndex[uGuid] = 1
    end
    local myEvents = tEvents[uGuid]
    myEvents.eStartNext = Event.Create(Event.TimerRelative, {1.5, true}, StartNextCue, {uGuid})
    tEvents[uGuid] = myEvents
  end
end

function StartNextCue(uGuid)
  local uSpeaker = tSpeakers[uGuid]
  if uSpeaker then
    local nSpeechIndex = tSpeechIndex[uGuid]
    VO.CueWithoutSubtitles(uSpeaker, tSpeech[nSpeechIndex], CueFinished, {uGuid}, false, false)
  else
    Debug.Printf("Starting cue on non-existent speaker")
  end
  local myEvents = tEvents[uGuid]
  myEvents.eStartNext = nil
  tEvents[uGuid] = myEvents
end

function CancelCue(uGuid)
  local uSpeaker = tSpeakers[uGuid]
  if uSpeaker then
    local nSpeechIndex = tSpeechIndex[uGuid]
    VO.Cancel(uSpeaker, tSpeech[nSpeechIndex])
  end
  local myEvents = tEvents[uGuid]
  if myEvents.eStartNext then
    Event.Delete(myEvents.eStartNext)
    myEvents.eStartNext = nil
  end
  tEvents[uGuid] = myEvents
end
