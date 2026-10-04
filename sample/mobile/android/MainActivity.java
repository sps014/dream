package org.example.dreamvalidation;

import android.app.Activity;
import android.os.Bundle;
import android.util.Log;
import android.widget.TextView;

public final class MainActivity extends Activity {
    @Override
    public void onCreate(Bundle state) {
        super.onCreate(state);
        DreamLibrary.attach();
        int answer;
        try {
            answer = DreamLibrary.call_answer(20);
        } finally {
            DreamLibrary.detach();
        }
        if (answer != 42) {
            throw new AssertionError("Dream returned " + answer);
        }
        TextView label = new TextView(this);
        label.setText("Dream staticlib returned " + answer);
        setContentView(label);
        Log.i("DreamValidation", "DREAM_MOBILE_PASS answer=" + answer);
    }
}
