function OnActivate(uGateGuid, args)
  Event.Create(Event.ObjectHibernation, {uGateGuid, "awake"}, OpenGate, {uGateGuid})
end

function OpenGate(uGateGuid)
  Event.Create(Event.TimerRelative, {2}, Object.OpenGate, {uGateGuid})
end
